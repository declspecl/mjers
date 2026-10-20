//! Thin Python boundary; rules and observation encoding live in mjers-core.

use std::sync::Arc;

use mjers_core::advisor::heuristic;
use mjers_core::bot::encoding::{
    self,
    EncodedObservation,
};
use mjers_core::bot::policy::Policy;
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game;
use mjers_core::game::rng::derive_seed;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use numpy::ndarray::Array2;
use numpy::{
    IntoPyArray,
    PyArray1,
    PyArray2,
};
use pyo3::exceptions::{
    PyRuntimeError,
    PyValueError,
};
use pyo3::prelude::*;

/// Inputs for one seat's decision. Array writes never affect the game.
#[pyclass(frozen, module = "mjers._native", get_all)]
struct Observation {
    seat: usize,
    tiles: Py<PyArray2<f32>>,
    context: Py<PyArray1<f32>>,
    action_mask: Py<PyArray1<bool>>,
}

impl Observation {
    fn new(
        py: Python<'_>,
        seat: Seat,
        encoded: EncodedObservation,
    ) -> Self {
        Self {
            seat: seat.index(),
            tiles: Array2::from_shape_fn((encoding::PLANES, mjers_core::tile::KINDS), |(plane, tile)| encoded.tiles[plane][tile])
                .into_pyarray(py)
                .unbind(),
            context: encoded.context.to_vec().into_pyarray(py).unbind(),
            action_mask: encoded.action_mask.to_vec().into_pyarray(py).unbind(),
        }
    }
}

/// Rewards are zero until termination, then settlement units in absolute
/// seat order. The next observation may belong to a different seat.
#[pyclass(frozen, module = "mjers._native", get_all)]
struct Transition {
    observation: Option<Py<Observation>>,
    rewards: [i32; 4],
    terminated: bool,
}

/// Observation-only heuristic analysis for diagnostics, never policy inputs.
#[pyclass(frozen, module = "mjers._native", get_all)]
struct ActionAssessment {
    index: usize,
    description: String,
    shanten: i8,
    live_improving: u32,
    danger: f64,
    immediate_units: u32,
    reasons: Vec<String>,
}

/// A turn-based four-player environment, with no auto-reset.
/// Claim responses are separate steps; they remain private until resolution.
/// Use each environment on its creating Python thread.
#[pyclass(unsendable, module = "mjers._native")]
struct Env {
    game: Game,
    teacher: BotSpec,
    policies: [Box<dyn Policy>; 4],
}

#[pymethods]
impl Env {
    /// Teacher and rules accept preset names or their respective JSON objects.
    #[new]
    #[pyo3(signature = (teacher, *, rules))]
    fn new(
        teacher: &str,
        rules: &str,
    ) -> PyResult<Self> {
        let spec = BotSpec::preset(teacher)
            .map(Ok)
            .unwrap_or_else(|| serde_json::from_str(teacher))
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let policies = std::array::from_fn(|i| spec.build(derive_seed(0, i as u64)));
        let rules = Ruleset::preset(rules)
            .map(Ok)
            .unwrap_or_else(|| serde_json::from_str(rules))
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let game = Game::new(Arc::new(rules), 0, Seat::ALL[0]).map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { game, teacher: spec, policies })
    }

    #[getter]
    fn rules_json(&self) -> PyResult<String> {
        serde_json::to_string(self.game.rules()).map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    #[getter]
    fn teacher_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.teacher).map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    #[getter]
    fn concealed_hand(&self) -> PyResult<String> {
        let seat = self.current_seat()?;
        Ok(self.game.observe(seat).hand().concealed().to_string())
    }

    /// Existing advisor rankings, restricted to the acting seat's observation.
    /// Claims assume the advisor's best follow-up discard; these scores are
    /// explanatory proxies, not rollout values or a teacher-policy ranking.
    fn rank_actions(&self) -> PyResult<Vec<ActionAssessment>> {
        let seat = self.current_seat()?;
        let observation = self.game.observe(seat);
        heuristic::rank(&observation)
            .into_iter()
            .map(|assessment| {
                let index = encoding::action_index(assessment.action, observation.decision()).ok_or_else(|| PyRuntimeError::new_err("advisor action has no encoding"))?;
                Ok(ActionAssessment {
                    index,
                    description: assessment.action.to_string(),
                    shanten: assessment.shanten,
                    live_improving: assessment.live_improving,
                    danger: assessment.danger,
                    immediate_units: assessment.immediate_units,
                    reasons: assessment.reasons,
                })
            })
            .collect()
    }

    #[pyo3(signature = (seed, dealer))]
    fn reset(
        &mut self,
        py: Python<'_>,
        seed: u64,
        dealer: usize,
    ) -> PyResult<Option<Observation>> {
        let dealer = Seat::new(dealer).ok_or_else(|| PyValueError::new_err("dealer must be in 0..4"))?;
        let game = Game::new(Arc::new(self.game.rules().clone()), seed, dealer).map_err(|error| PyValueError::new_err(error.to_string()))?;
        self.game = game;
        self.policies = std::array::from_fn(|i| self.teacher.build(derive_seed(seed, i as u64)));
        Ok(self.observe(py))
    }

    fn observe(
        &self,
        py: Python<'_>,
    ) -> Option<Observation> {
        self.game.to_act().first().map(|seat| Observation::new(py, seat, encoding::encode(&self.game.observe(seat))))
    }

    /// Selects from the current legal mask. Illegal indices leave state intact.
    fn step(
        &mut self,
        py: Python<'_>,
        action: usize,
    ) -> PyResult<Transition> {
        let seat = self.current_seat()?;
        let action = encoding::legal_action(&self.game.observe(seat), action).ok_or_else(|| PyValueError::new_err("action index is not legal at this decision"))?;
        self.game.apply(seat, action).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        Ok(Transition {
            observation: self.observe(py).map(|observation| Py::new(py, observation)).transpose()?,
            rewards: self.game.settlement().map(|settlement| settlement.total()).unwrap_or([0; 4]),
            terminated: self.game.is_over(),
        })
    }

    /// Labels the current observation without advancing the game. Stateful
    /// teachers (random) consume their own RNG on each call.
    fn teacher_action(&mut self) -> PyResult<usize> {
        let seat = self.current_seat()?;
        let observation = self.game.observe(seat);
        let legal = observation.legal_actions();
        let action = self.policies[seat.index()].act(&observation, &legal);
        if !legal.contains(&action) {
            return Err(PyRuntimeError::new_err("teacher returned an illegal action"));
        }
        encoding::action_index(action, observation.decision()).ok_or_else(|| PyRuntimeError::new_err("teacher action has no encoding"))
    }
}

impl Env {
    fn current_seat(&self) -> PyResult<Seat> {
        self.game.to_act().first().ok_or_else(|| PyRuntimeError::new_err("hand is over; call reset"))
    }
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Env>()?;
    module.add_class::<Observation>()?;
    module.add_class::<Transition>()?;
    module.add_class::<ActionAssessment>()?;
    module.add("ENCODING_VERSION", encoding::VERSION)?;
    module.add("ACTION_COUNT", encoding::ACTIONS)?;
    module.add("TILE_PLANES", encoding::PLANES)?;
    module.add("CONTEXT_SIZE", encoding::CONTEXT)?;
    Ok(())
}
