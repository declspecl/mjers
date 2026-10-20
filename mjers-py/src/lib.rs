//! Thin Python boundary; rules and observation encoding live in mjers-core.

use std::collections::HashMap;
use std::sync::Arc;

use mjers_core::advisor::heuristic;
use mjers_core::bot::encoding::{
    self,
    EncodedObservation,
};
use mjers_core::bot::neural::{
    NeuralNetwork,
    ResidualNet,
    Weights,
};
use mjers_core::bot::policy::Policy;
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game;
use mjers_core::game::action::Action;
use mjers_core::game::rng::derive_seed;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use numpy::ndarray::{
    Array2,
    Array3,
};
use numpy::{
    IntoPyArray,
    PyArray1,
    PyArray2,
    PyArray3,
    PyReadonlyArrayDyn,
    PyUntypedArrayMethods,
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

/// Rust inference for a `suit-resnet-v2` state dict of float32 arrays.
#[pyclass(frozen, module = "mjers._native")]
struct NeuralNet {
    net: Arc<ResidualNet>,
}

#[pymethods]
impl NeuralNet {
    #[new]
    fn new(tensors: HashMap<String, PyReadonlyArrayDyn<'_, f32>>) -> PyResult<Self> {
        let mut weights = Weights::new();
        for (name, array) in tensors {
            let data = array.as_array().iter().copied().collect();
            weights.insert(name, array.shape().to_vec(), data).map_err(|error| PyValueError::new_err(error.to_string()))?;
        }
        let net = ResidualNet::new(&weights).map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { net: Arc::new(net) })
    }
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

    /// Logits (illegal actions at -inf) and predicted settlement units for the current observation.
    fn evaluate_network(
        &self,
        net: &NeuralNet,
    ) -> PyResult<(Vec<f32>, f32)> {
        let seat = self.current_seat()?;
        let evaluation = net.net.evaluate(&encoding::encode(&self.game.observe(seat)));
        Ok((evaluation.logits.to_vec(), evaluation.settlement))
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

/// One student seat per hand against a field playing in Rust. The field's
/// decisions, available wins and single-action decisions are applied without
/// returning to Python; only real student choices are reported, batched.
#[pyclass(unsendable, module = "mjers._native")]
struct Arena {
    rules: Arc<Ruleset>,
    field: BotSpec,
    slots: Vec<Option<Hand>>,
    finished: Vec<Finished>,
}

struct Hand {
    game: Game,
    seed: u64,
    seat: Seat,
    policies: [Box<dyn Policy>; 4],
}

/// A completed hand: its slot, hand seed, student seat and the student's settlement.
type Finished = (usize, u64, usize, i32);

/// Pending student decisions: slots, then their tiles, context and action masks.
type Pending = (Vec<usize>, Py<PyArray3<f32>>, Py<PyArray2<f32>>, Py<PyArray2<bool>>);

#[pymethods]
impl Arena {
    #[new]
    #[pyo3(signature = (field, *, rules, slots))]
    fn new(
        field: &str,
        rules: &str,
        slots: usize,
    ) -> PyResult<Self> {
        let Env { game, teacher, .. } = Env::new(field, rules)?;
        Ok(Self {
            rules: Arc::new(game.rules().clone()),
            field: teacher,
            slots: (0..slots).map(|_| None).collect(),
            finished: Vec::new(),
        })
    }

    #[getter]
    fn rules_json(&self) -> PyResult<String> {
        serde_json::to_string(&*self.rules).map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    /// Deals hand `seed` (dealer `seed % 4`, as in `Env`) into an empty slot.
    fn start(
        &mut self,
        slot: usize,
        seed: u64,
        seat: usize,
    ) -> PyResult<()> {
        let seat = Seat::new(seat).ok_or_else(|| PyValueError::new_err("seat must be in 0..4"))?;
        let entry = self.slots.get_mut(slot).ok_or_else(|| PyValueError::new_err("no such slot"))?;
        if entry.is_some() {
            return Err(PyValueError::new_err("slot is still playing a hand"));
        }
        let dealer = Seat::ALL[(seed % 4) as usize];
        let game = Game::new(self.rules.clone(), seed, dealer).map_err(|error| PyValueError::new_err(error.to_string()))?;
        let policies = std::array::from_fn(|i| self.field.build(derive_seed(seed, i as u64)));
        *entry = Some(Hand { game, seed, seat, policies });
        self.advance(slot)
    }

    /// Every slot whose student must choose, in slot order.
    fn pending(
        &self,
        py: Python<'_>,
    ) -> Pending {
        let waiting: Vec<(usize, EncodedObservation)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, hand)| hand.as_ref().map(|hand| (slot, encoding::encode(&hand.game.observe(hand.seat)))))
            .collect();
        let n = waiting.len();
        let tiles = Array3::from_shape_fn((n, encoding::PLANES, mjers_core::tile::KINDS), |(row, plane, tile)| waiting[row].1.tiles[plane][tile]);
        let context = Array2::from_shape_fn((n, encoding::CONTEXT), |(row, i)| waiting[row].1.context[i]);
        let mask = Array2::from_shape_fn((n, encoding::ACTIONS), |(row, i)| waiting[row].1.action_mask[i]);
        (
            waiting.iter().map(|&(slot, _)| slot).collect(),
            tiles.into_pyarray(py).unbind(),
            context.into_pyarray(py).unbind(),
            mask.into_pyarray(py).unbind(),
        )
    }

    /// Applies one student action per listed slot, then plays on to its next choice.
    fn act(
        &mut self,
        slots: Vec<usize>,
        actions: Vec<usize>,
    ) -> PyResult<()> {
        if slots.len() != actions.len() {
            return Err(PyValueError::new_err("one action per slot"));
        }
        for (slot, index) in slots.into_iter().zip(actions) {
            let hand = self.slots.get_mut(slot).and_then(Option::as_mut).ok_or_else(|| PyValueError::new_err("slot has no hand"))?;
            let observation = hand.game.observe(hand.seat);
            let action = encoding::legal_action(&observation, index).ok_or_else(|| PyValueError::new_err("action index is not legal at this decision"))?;
            hand.game.apply(hand.seat, action).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            self.advance(slot)?;
        }
        Ok(())
    }

    /// From now on, new hands' field seats play `net` (for example a frozen copy
    /// of the student). Hands already dealt keep their players.
    fn set_field_network(
        &mut self,
        name: &str,
        net: &NeuralNet,
    ) {
        self.field = BotSpec::Neural {
            name: name.into(),
            network: NeuralNetwork::new(name, (*net.net).clone()),
        };
    }

    /// Hands completed since the last call: (slot, seed, seat, student units).
    fn take_finished(&mut self) -> Vec<Finished> {
        std::mem::take(&mut self.finished)
    }
}

impl Arena {
    fn advance(
        &mut self,
        slot: usize,
    ) -> PyResult<()> {
        let Some(hand) = self.slots.get_mut(slot).and_then(Option::as_mut) else { return Ok(()) };
        while let Some(seat) = hand.game.to_act().first() {
            let observation = hand.game.observe(seat);
            let legal = observation.legal_actions();
            let action = if seat == hand.seat {
                let win = [Action::Win, Action::SelfDrawWin].into_iter().find(|win| legal.contains(win));
                match (win, legal.first()) {
                    (Some(win), _) => win,
                    (None, Some(&only)) if legal.len() == 1 => only,
                    _ => return Ok(()),
                }
            } else {
                hand.policies[seat.index()].act(&observation, &legal)
            };
            hand.game.apply(seat, action).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        }
        let units = hand
            .game
            .settlement()
            .map(|settlement| settlement.total()[hand.seat.index()])
            .ok_or_else(|| PyRuntimeError::new_err("hand ended without a settlement"))?;
        self.finished.push((slot, hand.seed, hand.seat.index(), units));
        self.slots[slot] = None;
        Ok(())
    }
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Env>()?;
    module.add_class::<Observation>()?;
    module.add_class::<Transition>()?;
    module.add_class::<ActionAssessment>()?;
    module.add_class::<NeuralNet>()?;
    module.add_class::<Arena>()?;
    module.add("ENCODING_VERSION", encoding::VERSION)?;
    module.add("ACTION_COUNT", encoding::ACTIONS)?;
    module.add("TILE_PLANES", encoding::PLANES)?;
    module.add("CONTEXT_SIZE", encoding::CONTEXT)?;
    Ok(())
}
