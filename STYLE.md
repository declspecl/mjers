# STYLE

This guide captures the preferences established through actual code reviews in this project and observations from the Rust projects ssolvent and monopolez. It is intended for anyone writing, reviewing, or generating code here. The principles are language agnostic; the names and examples keep the guidance specific.

Explicit feedback takes precedence over inferred habits. A current implementation can contain unfinished work or a shortcut that needs correcting; its existence does not make it an endorsed pattern. Apply these preferences with judgment, and follow newer instructions when they change.

## The overall shape

I want code that makes its purpose and organization obvious. Small, declarative, strongly typed pieces should expose the domain clearly and keep dependency details at the edges.

The desired result is a diff I can read as one coherent change: the names agree, the responsibilities are distinct, the formatting helps me follow the execution, and the implementation has been verified locally.

Simplicity includes the amount of machinery introduced. A short call site does not compensate for adopting a framework, creating a registry, or adding several layers to support one small operation.

## Model our domain

Start with the actual things our application manages and the operations we perform on them.

- A course resource is a thing. “Storage” describes a capability or implementation concern.
- A course resource file belongs to our domain. An S3 object key belongs to the S3 implementation.
- Course and resource identifiers describe relationships we own. Bucket names, object locations, and provider request fields do not belong in the core entity.
- Course resources can be related to lectures without becoming subordinate to lectures in every context.

Use domain identities at repository boundaries. An operation to upload or fetch a resource file should receive a meaningful resource reference, rather than asking its caller to manufacture a storage key.

Do not make core models resemble SDK request objects just because that makes an adapter easier to write. Translate between the two at the boundary.

Also resist inventing domain concepts to accommodate the implementation. Do not add an extra asset identifier when an existing resource identifier already identifies the file. Do not duplicate a filename as a title, original filename, and display name without distinct requirements for all three.

## Keep related domains close without excessive nesting

Organize by meaningful domain boundaries, with shallow directories and directly discoverable files.

The organization discussed for this project is:

```text
core/
    course/
    course-resource/
        file/
```

This expresses the relationship without turning every association into another directory level. A structure such as `core/course/resource/file` is already deeper than I want for these domains.

A course resource deserves a sibling domain even though it belongs to a course. File operations can form a small subdomain within course resources. Neither decision requires a generic asset framework or a reusable storage hierarchy.

Keep a small domain understandable from a handful of well-named files. Add folders when a real grouping emerges, rather than preallocating layers for hypothetical future complexity.

## Services, repositories, and facades have different jobs

These are responsibility boundaries for application and dependency integration, not a mandatory set of classes for every problem. A solver or game engine can express its domain clearly through value types, functions, and behavioral contracts. Add the roles the problem needs.

| Role                            | Responsibility                                                                                                        |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Model                           | Describe our entities, values, requests, results, and valid domain shapes.                                            |
| Service                         | Validate inputs, enforce business rules, coordinate operations, and interpret outcomes.                               |
| Repository                      | Define the domain contract that implementations must satisfy.                                                         |
| Facade                          | Make the dependency call and convert requests, responses, and failures between dependency and domain representations. |
| Runtime or composition boundary | Construct concrete dependencies and connect them to the application.                                                  |

Where services and facades are used, facades should be **very thin**. Their job is dependency interaction and conversion. Business policy and orchestration belong in the service or corresponding domain operations.

For example, the service decides which upload states allow completion or failure. The persistence facade can execute an atomic conditional update using those allowed states. That supplies the concurrency mechanism without making SQL the owner of the transition policy.

An SDK adapter should not absorb validation, cancellation policy, timeouts, result interpretation, or orchestration merely because it is the class nearest the SDK.

Keep repository contracts honest. Mock and production implementations should support the same abstraction when they are presented as implementations of that repository. Removing the implementation declaration to avoid missing methods hides an architectural mismatch.

Lecture aggregation belongs in business or application composition. A Postgres course facade should not fetch lectures to assemble a screen response. A mock adapter that also assembles that response needs its responsibilities made explicit too.

## Be pure with dependency injection

External dependencies are required inputs. Construct them at an explicit composition boundary and pass them in.

The rejected pattern was a facade accepting an optional client and silently constructing a real client when none was supplied. Even if that fallback looks convenient, it defeats the boundary.

- A facade receives its client. It does not choose whether to create one.
- A service receives its repository. It does not instantiate a particular facade.
- Core code does not look up environment variables or reach for a global dependency as an alternative to injection.
- Tests substitute dependencies through the same contract used by production code.
- Runtime ownership of a connection pool or client is explicit.

In languages with classes, the existing style favors explicit private dependency fields, immutable references where possible, and visible constructor assignments. Keep construction easy to inspect.

An algorithm can achieve the same transparency through explicit state, rules, seeds, and strategy parameters. Ordinary local value construction does not require a factory or injected interface. The game engine's payment operations illustrate this distinction.

Factories need to earn their existence. A function whose only purpose is to forward arguments into one constructor is usually unnecessary redundancy. The former `create-llm-service` wrapper was an example I questioned.

## Adopt packages at the size of the problem

“Adopt packages. Do not be shy” and “that is way too heavy” are both part of this style.

Use a dependency when it removes substantial integration work, delicate plumbing, or a genuine maintenance burden. Judge its cost against the specific capability being requested.

| Problem we discussed                                  | Preferred scale of solution                                            |
| ----------------------------------------------------- | ---------------------------------------------------------------------- |
| Support multiple LLM providers                        | Provider SDKs and a useful shared SDK, behind our repository contract. |
| Validate runtime data while preserving useful types   | A schema library, using a slim API where it meets the need.            |
| Deadline and cancellation plumbing                    | A focused package such as the timeout helper we adopted.               |
| Describe a few database columns declaratively         | A small static mapping checked against known model fields.             |
| Give three opaque database error codes readable names | Local named constants with a source reference.                         |

A request for more declarative, type-safe configuration is not permission to adopt an ORM. Drizzle and an error-code package were rejected when a hand-written static configuration was the intended change.

Do not respond to every concern with another package, and do not respond to package avoidance by building a miniature framework ourselves. Prefer the smallest implementation that actually handles the requirement cleanly.

Use current, supported dependency APIs. When a response property is deprecated in favor of the final step's response, use the supported property rather than carrying the deprecated access forward.

## Make types carry information

Python-specific requirements established on 2026-10-20:

- Strongly type all owned Python code, including tests and native extension stubs.
- Every function parameter is required. Do not introduce optional configuration
  parameters or default argument values; make choices explicit at call sites.
- Use typed records for dataset, configuration and result shapes. Specify NumPy
  dtypes, and validate tensor/array dimensions at data boundaries where ordinary
  Python annotations cannot express them.
- Run strict Mypy, Ruff lint and Ruff formatting checks. Do not hide failures with
  `Any`, casts or ignored diagnostics. Keep genuinely dynamic library/file APIs at
  explicit, narrow boundaries.
- Keep Python packaging and uv configuration at the repository root.

Types should describe the actual set of values and preserve relationships between inputs and outputs.

- Infer model types from schemas where the language and tooling support it.
- Keep generic output types intact through the layers that can preserve them.
- Use explicit environment/configuration shapes with the fields we actually read.
- Describe finite sets with closed types and exhaustive mappings where appropriate.
- Make optionality and absence deliberate.
- Keep dependency representations inside dependency-facing code.

The objection to “untyped records everywhere” is about replacing a known structure with a catch-all map. A finite mapping keyed by the fields of a known model is useful; an arbitrary string-keyed bag discards information we already have.

Read known configuration fields explicitly. A prefix registry or synthesized environment-variable name needs a clearer benefit than avoiding two straightforward field lookups.

Likewise, an unknown value can be appropriate at an untrusted input or exception boundary. Narrow it there. Do not propagate an unknown structured output through our core API when the schema and generic parameter already tell us its type.

Do not fix type errors with an escape hatch that erases the problem. Broad dynamic types, double casts, and assertions through an unknown type weaken the contract rather than repair it.

A database column map can be static and checked for complete model coverage. It does not need a schema framework, code generator, or generic query system to become useful.

Semantic aliases can improve readability without creating distinct identities in the type system. Recognize that difference. Use stronger value types when enforcing a distinction matters; do not require a wrapper around every primitive merely to increase the number of types.

## Make representation and cost part of the design

Data representation is a design decision. Consider the operations, bounds, allocation behavior, and amount of state the workload actually needs.

- Fixed domains can use fixed arrays and dimensions known ahead of execution.
- Small bounded sets can use bitsets, with named operations that explain their domain meaning.
- Integer widths should reflect known bounds, with the assumptions visible.
- Immutable relationships can be computed once into lookup tables.
- A concrete iterator can avoid allocation or dynamic dispatch while keeping iteration readable.
- Small, self-contained state can make branching, snapshots, and replay straightforward.
- Alignment and layout assertions can be useful when memory behavior materially affects the workload.

The candidate set and iterator give a compact representation domain operations such as membership and removal. The iterator wrapper has an explicit allocation-related purpose. Monopoly's board state makes array dimensions and memory layout visible.

Judge a wrapper by what it contributes, rather than its field count. A one-field value type can establish useful semantics, implement an iterator, or control representation. A forwarding layer that contributes none of those things still needs justification.

Use these techniques where they fit the workload. Do not introduce cache-oriented layouts into an ordinary persistence change without a reason. Benchmark relevant hot paths when performance motivates a design choice, and keep its costs understandable at the call site.

## Validate real requirements

Validation should enforce requirements we can explain.

The repeated concern was unnecessary regex: identifier patterns, elaborate MIME-type expressions, and similar restrictions added because they looked defensive.

Before introducing a rule, identify the requirement it represents. A nonempty value, a supported size, or agreement between a byte buffer and its declared length can be meaningful. A restrictive character pattern for a domain identifier may simply be an invented limitation.

- Do not forbid punctuation or Unicode in filenames without a reason.
- Do not make core identifiers follow a provider's key format.
- Do not turn every text field into a miniature parser.
- Keep necessary provider encoding inside the facade.
- Avoid scattering equivalent validation across several layers.

Be precise about which layer owns the rule. Upload status, request validation, and file metadata completeness are API/domain contracts in the work reviewed here.

## Keep database structure separate from API policy

Use the database for persistence, structural types, keys, relationships, and uniqueness that the domain actually requires.

Do not automatically reproduce every API rule as a database constraint. The following checks were explicitly removed from the resource migration:

- Membership in the set of upload-status strings.
- All-or-none presence of file metadata.
- Nonempty resource names and content types.
- Application-supported content-length bounds.
- Uploaded-state metadata requirements.

Keep those rules in the appropriate schemas and services. Atomic persistence operations can still protect updates from races.

Do not invent a uniqueness requirement merely because a field resembles an identifier. A display code and an entity's identity or URL slug can have different purposes. Resolve their meaning explicitly.

Provider-specific representations are conversion concerns. Examples include numeric database identifiers exposed as domain strings and large database integers converted into safe application numbers. Do those conversions locally; avoid changing a global parser for one facade's needs.

## Migrations should be small and unsurprising

- Use the existing sequential pattern: `000_...`, `001_...`, `002_...`.
- Do not introduce timestamp filenames into that convention.
- Add a new migration instead of rewriting an applied one.
- Limit the migration to the feature's actual structural changes.
- Preserve existing rows, identities, timestamps, and relationships.
- Represent missing legacy metadata honestly rather than inventing a backfill.
- Verify both an empty database and an upgrade containing existing data.

A persistence foundation can be a focused change. It does not need to bring in production routes, upload orchestration, processing jobs, and infrastructure in the same diff.

## Names should say what the operation means here

Be specific to our domain without making every name needlessly long.

Examples that reflect the intended distinction:

- `CourseResourceFileRepository` describes the domain contract.
- `S3CourseResourceFileFacade` names the dependency implementation.
- `uploadResource`, `fetchResource`, and `deleteResource` describe service operations.
- Shorter `upload`, `fetch`, and `delete` methods can work inside an explicitly named file repository.
- `PostgresCourseFacade` identifies the adapter for course persistence.

Avoid importing dependency terminology into the core operation just because the dependency uses it. The earlier `put(key, upload)` file contract was rejected for this reason.

Do not rename a term solely because a dependency also uses it. We reconsidered the concern about `generate` and explicitly kept it for LLM operations. Judge the term in context.

Names should also keep related concepts distinct. Upload success is different from document extraction, indexing, generated content readiness, or overall lecture readiness. A single status should not quietly take on all those meanings.

For collections and lookup tables, name what indexes them. Names such as `owned_tiles_by_player_id`, `improvement_level_by_property_id`, and `BOX_ID_BY_POSITION_ID_LUT` communicate relationships that `items`, `values`, or `lookup` would hide. This pattern appears in both the board model and position lookup tables.

## Be consistent about casing

Within identifiers, the explicit preference is **`Llm`**, consistently: `LlmRepository`, `LlmService`, `LlmError`, and provider facade names built from the same spelling.

Do not mix `LLM` and `Llm` across equivalent symbols. Apply the same care to the other words in a name. Follow the host language's casing conventions while keeping the concept's spelling consistent.

Natural-language prose and PR titles can still use ordinary acronyms such as “LLM.” Identifier consistency is the concern here.

## Prefer declarative code that remains easy to read

Use explicit data and straightforward composition when the shape is static. A typed map, a schema, or a short transformation often explains the intention better than procedural bookkeeping.

Use collection transformations for data transformations. Use straightforward loops, mutable locals, and explicit indices when they explain a stateful algorithm or bounded representation more clearly. There is no language-agnostic ban on imperative iteration.

The Sudoku solver branches and backtracks; a simulation advances game state and accumulates results. Their loops express the work directly. Do not replace that structure with a functional chain merely to make the code appear more declarative. Solver, simulation loop.

Avoid nested ternaries. The chain translating length, content filtering, and other finish reasons into domain outcomes was an explicit example. A few ordinary branches or a well-scoped mapping are easier to follow.

Use guard clauses for invalid inputs and unsupported outcomes. Keep the successful path readable without making the reader mentally unwind several levels of nesting.

Do not extract a helper merely to make a method look shorter. Extract when it names a real operation, separates responsibilities, or makes repetition and complexity meaningfully smaller.

## Make dependency calls visually easy to follow

A facade method should make these steps apparent:

1. Construct the request.
2. Make the dependency call.
3. Translate dependency failures.
4. Convert the successful result into our model.

Prefer a named request prepared before the call. Leave visual space between the request object and execution. Keep error handling focused on the boundary it represents.

Separate a large row-conversion block and error-conversion block from a query helper when that makes the helper easier to read. A helper should not be a cramped container for request execution, nested error classification, field mapping, and validation all at once.

Use readable multiline SQL. Parameterize values. Keep static column names in a small typed configuration when that avoids repeated opaque strings and manually maintained projections.

## Give opaque literals meaning

An unexplained number or provider code makes the reader stop and decode it.

- Give provider error codes descriptive names.
- Link to the authoritative reference when the mapping is externally defined.
- Name meaningful limits and timeouts.
- Express a derived limit clearly when that communicates intent better than a large decimal literal; the signed 32-bit maximum, 2³¹ − 1, was a concrete example.
- Keep a fixed domain vocabulary visible in its domain types.

For the Postgres codes we discussed, the installed driver exposes a code string but no named constants. Three local constants and the official SQLSTATE reference are sufficient.

Do not create a constant for every readable literal as a ritual. The goal is to expose meaning and reduce accidental disagreement, not to force the reader to chase ordinary domain words through another file.

## Keep errors domain-owned

Use a small, deliberate set of error categories that callers can act on. Translate dependency exceptions into those categories at the adapter boundary.

Business validation failures, missing entities, invalid transitions, and provider failures have different meanings. Preserve useful distinctions without building an elaborate error framework.

Choose error structure for its consumers. A reusable engine can expose a typed action error; an application boundary can use contextual diagnostic errors. Do not manufacture a dedicated error hierarchy for every local helper. The engine action errors and simulation boundary serve different needs.

Keep messages safe and comprehensible. A domain error should not expose the query, raw provider response, or connection details as its public explanation.

Services should interpret meaningful outcomes from the repository. For example, a conditional update that matches nothing may require checking whether the entity disappeared, already reached the requested state, or moved into an incompatible state.

## Formatting should help the reader see the structure

Follow the project's formatter and language conventions. ClassIQ's current choices are concrete, but its punctuation and line width are not universal personal rules:

- Four spaces for indentation.
- Double-quoted strings where the language permits that choice.
- Explicit statement terminators and trailing delimiters where the language supports them and the formatter uses them.
- A target line width of 140 characters in this repository.
- Consistent, formatter-managed import ordering.
- Braces around ordinary conditional bodies in brace-based languages.
- Blank space before returns where it separates them from preceding work.
- Blank lines between validation, request construction, execution, and conversion when they are distinct steps.

Use the appropriate formatter for the host language rather than reproducing another language's punctuation mechanically.

For example, monopolez configures a 200-column width, vertical parameters and imports, and a 120-column function-call width. Its movement functions commonly use tail expressions, whereas ssolvent's core value methods use explicit returns. Preserve local consistency rather than imposing one spelling across languages or repositories.

Keep switch/case formatting simple. The rule requiring braces around every switch case was explicitly disabled. Use a scoped block when the code needs a scope, rather than adding blocks solely to satisfy that stylistic rule.

Avoid dense stretches of nested catches and conversions even when the formatter accepts them. Formatting is part of organization, not just automated punctuation.

## Use direct imports and restrained file boundaries

No barrel imports. Do not add an index file whose purpose is to re-export a directory's contents. Import the actual module directly.

An index-named entrypoint that performs real routing or composition is a different case; its role matters. So is a language's module-declaration file, such as a Rust `mod.rs` that declares child modules. The objection is to the re-export indirection.

Keep files purposeful. A model, service, repository, facade, and domain errors can each deserve a file when those roles exist. A forwarding factory, semantically empty wrapper, or directory of one-line indirections usually does not. Separating a domain's models, static data, and operations is also a useful organization; it does not require a separate service class for each operation.

Comments should explain an external reference, a non-obvious constraint, or a decision that the code cannot express clearly. Avoid narrating obvious assignments and restating method names.

## Verify behavior before trimming verification code

The preference for a clean final diff does not waive behavioral verification.

For particular ClassIQ changes, we have explicitly used this workflow:

1. Write meaningful tests or a local verification harness.
2. Exercise the implementation, including relevant failure paths.
3. Verify the behavior locally.
4. Trim or archive temporary verification scaffolding after it has passed.

Removing tests before establishing the behavior misses the point. Equally, the fact that a test was useful during implementation does not automatically mean every temporary test belongs in the shipped diff.

Preserve existing tests. Decide what new coverage should remain based on its lasting value and the agreed scope, rather than treating temporary-test removal as a blanket policy.

The Rust repositories retain both solver integration tests and tests beside simulation code. Permanent regression coverage and temporary implementation harnesses have different lifetimes. Keep tests that continue to protect behavior.

Use real dependency behavior when it matters. For persistence, meaningful evidence includes fresh and upgrade migrations, existing relationships, partial updates, conflicts, and relevant concurrent operations. For runtime packaging, inspect and execute the extracted artifact. An unchanged mock endpoint does not prove a new persistence layer works.

Run checks appropriate to the change. Report what was actually verified and distinguish mocked, local integration, offline infrastructure, and live-service testing accurately.

## Keep changes reviewable and publication explicit

Finish an authorized slice well enough that its diff is concrete and reviewable. Respect an explicit checkpoint where I want to inspect it before moving on.

Working on a branch, spawning parallel implementations, committing, pushing, and publishing a PR are separate actions. Do not infer permission to publish from a request to work in parallel or prepare a change. Once publication has been explicitly authorized, carry it through without repeatedly asking for the same permission.

Use isolated worktrees for parallel implementation when appropriate. Keep independent branches based on the intended common base. Stack a PR only when it genuinely depends on another change, as processing infrastructure can depend on runtime separation.

Check overlap and compatibility; do not simply promise that independently assigned tasks cannot conflict. Preserve existing work, stashes, and the user's checkout choices.

Reread the current files after I edit them. An earlier version, remembered implementation, or conversation summary is not a substitute for the code now on disk.

Before raising a PR, confirm that the intended changes have actually been pushed to its remote branch. A correct local diff is not proof that the PR contains it.

## Write commits and PRs in the same restrained style

Commit and PR titles should be short, concrete, plain-English descriptions of the change. Imperative titles such as “Add course metadata persistence” or “Separate backend runtime entrypoints” fit. Do not introduce a conventional-commit prefix as a requirement when that is not the established style.

Use the repository PR template. The established shape is:

```markdown
## Summary

<task or issue link>

A concise description of the behavior or abstraction added.

## Notes

- A few decisions a reviewer needs to understand.

## Testing

- What was actually verified.
```

Put the task or issue links **above the summary prose**, under the Summary heading. Keep the summary short and direct. Notes explain the organization or meaningful tradeoffs; Testing gives relevant evidence.

Describe the final implementation, not the conversation that produced it. Do not include abandoned approaches, giant verification inventories, or implementation trivia that a reviewer does not need.

Review the whole diff and commit message together, rather than only checking the last fix. The names, file organization, contracts, title, and description should describe one coherent final change.

Be exact about scope. Shared persistence abstractions are not production HTTP integration. A foundation that contributes to a broad issue should not claim to complete the entire issue.

## The questions to ask before handing me a diff

- Are these our entities and operations, or dependency concepts wearing domain names?
- Can I replace the provider without rewriting the business logic?
- Are all dependencies explicit, including mocks?
- Does every implementation actually satisfy the contract it claims to implement?
- Is this a small static configuration problem that has accidentally become a framework?
- Do the chosen representation and abstraction costs fit the workload?
- Are collection dimensions and indexing relationships clear from their types and names?
- Do the types preserve information we already know?
- Did I introduce a regex, constraint, identifier, wrapper, or helper without a concrete requirement?
- Are names and acronym casing consistent throughout the change?
- Am I following this project's formatting and language idioms rather than importing another project's habits?
- Can I follow request construction, execution, errors, and conversion without decoding a dense block?
- Does the migration follow our numbering and preserve existing data honestly?
- Have I verified the behavior, even if temporary tests were removed afterward?
- Does the published branch contain the reviewed changes, and was publication actually authorized?

The standard is code I can understand, own, and change without fighting the abstractions around it.
