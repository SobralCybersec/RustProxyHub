Current: Universal Engineering Checklist / consolidated to exactly 50 sections / researched quality-gate tooling appended at the end.

# Universal Engineering Checklist — 50 Engineering Quality Gates

> **Primary constraint:** Never break working behavior. Every implemented change must be verified through tests, builds, static analysis, benchmarks, or explicit manual validation before being marked complete.

## Legend

* `[x]` Implemented and verified
* `[=]` Evaluated; no change warranted
* `[~]` In progress
* `[ ]` Not started
* `✅` Already satisfied

---

# 1. Dependency Policy

* [ ] Every dependency has a documented or obvious purpose.
* [ ] Prefer the standard library before introducing dependencies.
* [ ] Reuse existing project dependencies before adding equivalent functionality.
* [ ] Remove unused dependencies.
* [ ] Remove duplicate dependencies providing the same capability.
* [ ] Avoid unnecessary transitive dependency growth.
* [ ] Disable unnecessary default features.
* [ ] Keep optional functionality behind feature flags.
* [ ] Review dependencies periodically.
* [ ] Monitor dependency vulnerabilities.
* [ ] Monitor abandoned/unmaintained dependencies.
* [ ] Review dependency licenses.
* [ ] Keep lockfiles committed.
* [ ] Pin versions where reproducibility requires it.
* [ ] Avoid dependencies leaking unnecessarily into public APIs.

---

# 2. Code Style

* [ ] Formatter enforced.
* [ ] Formatter enforced in CI.
* [ ] Linter warnings treated as failures.
* [ ] Compiler warnings treated as failures.
* [ ] Remove dead imports.
* [ ] Remove unused variables.
* [ ] Remove unreachable code.
* [ ] Prefer immutable values.
* [ ] Minimize mutable state.
* [ ] Use clear names.
* [ ] Avoid misleading abbreviations.
* [ ] Avoid magic numbers.
* [ ] Prefer explicit types at boundaries.
* [ ] Comments explain *why*, constraints, or invariants rather than restating code.
* [ ] Formatting is deterministic.
* [ ] Generated files are excluded where appropriate.

---

# 3. Code Quality

* [ ] Code has one obvious purpose.
* [ ] Code is simpler than the problem it solves.
* [ ] Functions operate at a consistent abstraction level.
* [ ] Side effects are explicit.
* [ ] State transitions are explicit.
* [ ] Invalid states are difficult to represent.
* [ ] Public APIs are intentional.
* [ ] Implementation details remain private.
* [ ] Similar problems use similar solutions.
* [ ] Avoid unnecessary wrappers.
* [ ] Avoid unnecessary traits/interfaces.
* [ ] Avoid unnecessary inheritance.
* [ ] Avoid unnecessary polymorphism.
* [ ] Avoid unnecessary macros/metaprogramming.
* [ ] Avoid speculative abstractions.
* [ ] Avoid clever code when straightforward code works.
* [ ] Temporary workarounds have removal conditions.

---

# 4. Readability

* [ ] Names reveal intent.
* [ ] Domain terminology is consistent.
* [ ] Avoid ambiguous abbreviations.
* [ ] Avoid single-letter variables outside trivial local contexts.
* [ ] Complex expressions are decomposed.
* [ ] Boolean expressions remain understandable.
* [ ] Double negatives are avoided.
* [ ] Happy path is visually obvious.
* [ ] Failure path is visually obvious.
* [ ] Related code stays physically close.
* [ ] Functions read naturally top-to-bottom.
* [ ] Hidden mutation is avoided.
* [ ] Units appear in types or names where relevant.
* [ ] Guard clauses reduce nesting.
* [ ] A new contributor can understand the code without reconstructing hidden assumptions.

---

# 5. Reliability

* [ ] Expected failure modes are identified.
* [ ] Partial failures preserve valid state.
* [ ] Critical multi-step operations are transactional where appropriate.
* [ ] Operations are idempotent where required.
* [ ] External operations have timeouts.
* [ ] Retries are bounded.
* [ ] Retries use appropriate backoff.
* [ ] Non-idempotent operations are not blindly retried.
* [ ] Cancellation preserves consistency.
* [ ] Graceful shutdown preserves consistency.
* [ ] Resource exhaustion is handled.
* [ ] Queue overload is handled.
* [ ] Network interruptions are handled.
* [ ] Invalid external data cannot crash unrelated components.
* [ ] Startup failures provide actionable diagnostics.
* [ ] Recovery behavior is tested.

---

# 6. Testability

* [ ] Domain logic can run without starting the whole application.
* [ ] External dependencies are isolated at boundaries.
* [ ] Time can be controlled in tests.
* [ ] Randomness can be controlled.
* [ ] Filesystem access can be substituted where useful.
* [ ] Network calls can be substituted where useful.
* [ ] Database behavior can be integration-tested independently.
* [ ] Pure functions are preferred for deterministic logic.
* [ ] Constructors avoid surprising I/O.
* [ ] Hidden global state is avoided.
* [ ] Business logic is independent of UI components.
* [ ] Business logic is independent of transport representation.
* [ ] Tests validate behavior rather than implementation details.
* [ ] Tests are deterministic.
* [ ] Test ordering does not matter.
* [ ] Tests can run in parallel where practical.

---

# 7. Maintainability

* [ ] Changes remain localized.
* [ ] Components expose small interfaces.
* [ ] Internal implementation is encapsulated.
* [ ] Configuration is centralized where appropriate.
* [ ] Dependencies point in understandable directions.
* [ ] Feature removal is straightforward.
* [ ] Deprecated code has a removal strategy.
* [ ] Compatibility layers have expiration conditions.
* [ ] Important design decisions are documented.
* [ ] Cross-cutting concerns are consistently implemented.
* [ ] One conceptual change does not require edits across unrelated modules.
* [ ] Maintenance burden is considered before introducing features.
* [ ] Change amplification is monitored.

---

# 8. Efficiency

* [ ] Time complexity is appropriate for expected input size.
* [ ] Space complexity is appropriate.
* [ ] Repeated expensive work is avoided.
* [ ] Avoid unnecessary cloning/copying.
* [ ] Avoid unnecessary allocations.
* [ ] Avoid unnecessary serialization/deserialization.
* [ ] Avoid unnecessary DB round trips.
* [ ] Avoid unnecessary network round trips.
* [ ] Avoid unnecessary filesystem I/O.
* [ ] Avoid unnecessary locking.
* [ ] Avoid unnecessary polling.
* [ ] Avoid loading unused data.
* [ ] Stream large inputs when appropriate.
* [ ] Batch operations where appropriate.
* [ ] Unbounded queues/collections/caches are avoided.

---

# 9. Error Handling

* [ ] Use typed domain errors where practical.
* [ ] Avoid opaque string-only errors.
* [ ] Propagate errors correctly.
* [ ] Recover only where meaningful.
* [ ] Do not silently swallow failures.
* [ ] User-facing errors are understandable.
* [ ] Internal diagnostics preserve technical detail.
* [ ] Error context is added at useful boundaries.
* [ ] Root causes are preserved.
* [ ] Error messages do not leak secrets.
* [ ] Errors are actionable.
* [ ] Error paths are tested.

---

# 10. Module Architecture

* [ ] Modules follow Single Responsibility.
* [ ] High cohesion.
* [ ] Low coupling.
* [ ] Feature-oriented organization where beneficial.
* [ ] Avoid god modules.
* [ ] Avoid dumping grounds such as oversized `utils`, `common`, or `helpers`.
* [ ] Avoid cyclic dependencies.
* [ ] Internal details remain internal.
* [ ] Public module surface is intentionally small.
* [ ] Module names describe actual responsibility.
* [ ] Architecture reflects domain boundaries.

---

# 11. Composition

* [ ] Prefer composition over inheritance.
* [ ] Dependencies are injected where useful.
* [ ] Composition root is centralized.
* [ ] Avoid unnecessary globals.
* [ ] Infrastructure is assembled outside domain logic.
* [ ] Runtime implementation selection is explicit.
* [ ] Lifecycle ownership is obvious.
* [ ] Avoid service-locator patterns where normal dependency flow is sufficient.

---

# 12. Design Patterns

Apply patterns only where justified.

## NewType

* [ ] IDs.
* [ ] Units.
* [ ] Domain values.
* [ ] Parameter mix-up prevention.

## Builder

* [ ] Complex configuration.
* [ ] Many optional parameters.
* [ ] Construction genuinely becomes clearer.

## Factory

* [ ] Multiple runtime implementations.
* [ ] Creation logic genuinely varies.

## RAII / deterministic cleanup

* [ ] Files.
* [ ] Locks.
* [ ] Transactions.
* [ ] Temporary resources.
* [ ] Connections.

## TypeState

* [ ] Compile-time state safety where runtime dynamism is unnecessary.

Evaluate:

* [ ] Builder.
* [ ] Factory.
* [ ] Strategy.
* [ ] Observer.
* [ ] State.
* [ ] TypeState.
* [ ] Visitor.
* [ ] Adapter.

If the pattern adds more complexity than it removes:

* [ ] Do not introduce it.

---

# 13. Database

* [ ] Schema migrations.
* [ ] Prepared/parameterized statements.
* [ ] Transactions.
* [ ] Appropriate indexes.
* [ ] Full-text search where justified.
* [ ] No avoidable N+1 queries.
* [ ] Query arguments grouped meaningfully.
* [ ] Connection pooling/management.
* [ ] Input/data validation.
* [ ] Constraints enforce important invariants.
* [ ] Query plans reviewed for hot paths.
* [ ] Backup strategy.
* [ ] Migration rollback/forward strategy.
* [ ] Data migrations are tested.

---

# 14. API / IPC

* [ ] Typed DTOs.
* [ ] No `any` at typed boundaries.
* [ ] Boundary validation.
* [ ] Stable payload shape.
* [ ] Serialization consistency.
* [ ] Contracts versioned when public compatibility requires it.
* [ ] Unknown input is handled deliberately.
* [ ] Errors use consistent structure.
* [ ] Optional/nullable semantics are explicit.
* [ ] Internal domain models are not leaked accidentally.
* [ ] Payload size is bounded where necessary.

---

# 15. Real-Time Systems

Evaluate the simplest mechanism that satisfies the requirement:

* [ ] SSE.
* [ ] WebSockets.
* [ ] Long polling.
* [ ] Normal polling.
* [ ] Background workers.
* [ ] Message queue.
* [ ] Event bus.

Review:

* [ ] Reconnection.
* [ ] Ordering.
* [ ] Delivery guarantees.
* [ ] Backpressure.
* [ ] Cancellation.
* [ ] Dead connections.
* [ ] Resource limits.
* [ ] Duplicate event handling.

---

# 16. Concurrency

* [ ] Minimize shared mutable state.
* [ ] Prefer ownership/message passing.
* [ ] Avoid blocking async runtimes.
* [ ] Cancellation is supported.
* [ ] Graceful shutdown is implemented.
* [ ] Thread safety is explicit.
* [ ] Lock scope is minimized.
* [ ] Lock ordering is consistent.
* [ ] Deadlock risk is reviewed.
* [ ] Race conditions are tested where practical.
* [ ] Background tasks have ownership.
* [ ] Task failures cannot disappear silently.
* [ ] Unbounded task spawning is avoided.

---

# 17. Observability

## Logging

* [ ] Structured logs.
* [ ] Appropriate log levels.
* [ ] Capability/module targets.
* [ ] Correlation IDs.
* [ ] Request IDs.
* [ ] Sensitive information excluded.

## Tracing

* [ ] Meaningful spans.
* [ ] Performance timing.
* [ ] Context propagation.
* [ ] External calls traced.

## Metrics

* [ ] Errors.
* [ ] Latency.
* [ ] Throughput.
* [ ] Queue sizes.
* [ ] Resource utilization.
* [ ] Retry counts.
* [ ] Saturation.

---

# 18. Security

* [ ] Secrets are never committed.
* [ ] Secrets use appropriate secure storage.
* [ ] Keychain/keyring where appropriate.
* [ ] Validate untrusted input.
* [ ] Encode/escape output appropriately.
* [ ] Principle of least privilege.
* [ ] Secure defaults.
* [ ] Avoid leaking sensitive data.
* [ ] Passwords use established password hashing.
* [ ] Sensitive storage is encrypted when required.
* [ ] Dependencies are vulnerability-scanned.
* [ ] Authentication boundaries are explicit.
* [ ] Authorization is checked server-side.
* [ ] Security-sensitive events are observable.
* [ ] Unsafe functionality is minimized.

---

# 19. Performance

* [ ] Measure before optimizing.
* [ ] Establish baseline measurements.
* [ ] Benchmark hot paths.
* [ ] Profile before major optimization.
* [ ] Cache only where justified.
* [ ] Cache invalidation is defined.
* [ ] Lazy initialization where beneficial.
* [ ] Avoid unnecessary allocations.
* [ ] Avoid unnecessary copies.
* [ ] Avoid unnecessary parsing.
* [ ] Startup time is measured where important.
* [ ] Memory usage is measured where important.
* [ ] Performance-sensitive changes include before/after evidence.

---

# 20. Frontend

* [ ] Typed state.
* [ ] Typed IPC/API.
* [ ] No unjustified `any`.
* [ ] Remove dead components.
* [ ] Remove dead hooks.
* [ ] Feature organization where useful.
* [ ] Keep presentational components simple.
* [ ] Business logic lives outside rendering where practical.
* [ ] Accessibility.
* [ ] Keyboard navigation.
* [ ] Responsive layouts.
* [ ] Loading states.
* [ ] Error states.
* [ ] Empty states.
* [ ] Avoid unnecessary re-renders.
* [ ] Cleanup subscriptions/listeners/effects.
* [ ] UI state has one authoritative owner.

---

# 21. JavaScript / TypeScript / Node

* [ ] Strict TypeScript where practical.
* [ ] No unjustified explicit `any`.
* [ ] No unsafe implicit `any`.
* [ ] Remove dead scripts.
* [ ] Remove unused exports.
* [ ] Remove unused dependencies.
* [ ] Remove unused files.
* [ ] Detect dependency cycles.
* [ ] Shared helpers used only for genuinely shared behavior.
* [ ] Consistent async/error protocol.
* [ ] Rejected promises are handled.
* [ ] Runtime validation exists at trust boundaries.
* [ ] ESM/CJS behavior is intentional.
* [ ] Build tooling has a documented purpose.

---

# 22. Unit Testing

* [ ] Domain logic.
* [ ] Parsers.
* [ ] Validators.
* [ ] Transformations.
* [ ] Helpers with meaningful logic.
* [ ] State transitions.
* [ ] Boundary conditions.
* [ ] Failure cases.
* [ ] Edge cases.
* [ ] Tests remain fast enough for regular execution.

---

# 23. Integration Testing

* [ ] Database behavior.
* [ ] API contracts.
* [ ] IPC contracts.
* [ ] Serialization.
* [ ] Filesystem integration.
* [ ] External-service adapters.
* [ ] Authentication integration.
* [ ] Migrations.
* [ ] Transactions.
* [ ] Concurrency-sensitive integration where relevant.

---

# 24. End-to-End and Regression Testing

## End-to-End

* [ ] Main workflows.
* [ ] Critical user journeys.
* [ ] Browser automation where relevant.
* [ ] Desktop UI interaction where relevant.
* [ ] Import/export workflows.
* [ ] Startup/shutdown workflows.

## Regression

* [ ] Every fixed production bug receives a regression test where practical.
* [ ] Test fails before the fix.
* [ ] Test passes after the fix.
* [ ] Regression test targets externally observable behavior.
* [ ] Regression coverage remains permanently enabled.

---

# 25. CI/CD

CI must include as applicable:

* [ ] Dependency restore.
* [ ] Lockfile validation.
* [ ] Format check.
* [ ] Lint.
* [ ] Static analysis.
* [ ] Type check.
* [ ] Unit tests.
* [ ] Integration tests.
* [ ] E2E tests.
* [ ] Security scan.
* [ ] Dependency vulnerability scan.
* [ ] Duplication gate.
* [ ] Complexity gate.
* [ ] File-size gate.
* [ ] Debug build.
* [ ] Release build.
* [ ] Coverage reporting.
* [ ] Artifact verification.
* [ ] Multiple platforms where applicable.
* [ ] Dependency caching where beneficial.
* [ ] Required checks prevent merging when failing.

---

# 26. Documentation

* [ ] README.
* [ ] Setup guide.
* [ ] Architecture overview.
* [ ] Module responsibilities.
* [ ] Configuration.
* [ ] Environment variables.
* [ ] Event documentation.
* [ ] API/IPC documentation.
* [ ] Feature flags.
* [ ] Database migration procedure.
* [ ] Release procedure.
* [ ] Troubleshooting.
* [ ] Contribution guide.
* [ ] Significant architectural decisions.
* [ ] Documentation changes accompany behavioral changes.

---

# 27. Engineering Principles

Evaluate:

* [ ] DRY.
* [ ] KISS.
* [ ] YAGNI.
* [ ] SOLID.
* [ ] Law of Demeter.
* [ ] Fail Fast.
* [ ] Separation of Concerns.
* [ ] Principle of Least Astonishment.
* [ ] Encapsulation.
* [ ] Information hiding.
* [ ] Dependency inversion where actually useful.

The principles are heuristics rather than objectives to maximize independently.

---

# 28. Cleanup

* [ ] Remove dead code.
* [ ] Remove unused assets.
* [ ] Remove obsolete TODOs.
* [ ] Remove obsolete FIXME comments.
* [ ] Remove obsolete comments.
* [ ] Remove temporary debugging.
* [ ] Remove temporary logging.
* [ ] Remove obsolete feature flags.
* [ ] Remove duplicate implementations.
* [ ] Remove unused configuration.
* [ ] Remove abandoned migrations/scripts when appropriate.
* [ ] Remove compatibility paths after their lifetime expires.

---

# 29. Release Verification

## Manual

* [ ] Smoke test.
* [ ] Main workflows.
* [ ] Error handling.
* [ ] Settings.
* [ ] Import/export.
* [ ] Upgrade path.
* [ ] Startup/shutdown.

## Builds

* [ ] Debug.
* [ ] Release.
* [ ] Supported target platforms.

## Final

* [ ] CI green.
* [ ] No warnings.
* [ ] Version updated.
* [ ] Changelog updated.
* [ ] Release notes accurate.
* [ ] Release artifacts verified.
* [ ] Tag created only after verification.

---

# 30. Post-Release

* [ ] Monitor application errors.
* [ ] Monitor crashes.
* [ ] Monitor latency.
* [ ] Monitor resource usage.
* [ ] Monitor performance regressions.
* [ ] Monitor security findings.
* [ ] Monitor migration failures.
* [ ] Review user feedback.
* [ ] Review unexpected support volume.
* [ ] Schedule dependency review.
* [ ] Record lessons from incidents.

---

# 31. File and Module Size

## Hard rule

* [ ] **No hand-written source file exceeds 1000 lines.**

## Recommended thresholds

* [ ] Prefer files below **500 lines**.
* [ ] Review files above **500 lines**.
* [ ] Strongly consider refactoring above **750 lines**.
* [ ] Files reaching **1000 lines fail CI**.

Exceptions:

* [ ] Generated source.
* [ ] Generated bindings.
* [ ] Machine-generated schemas.
* [ ] Large declarative lookup data where splitting harms comprehension.

Exceptions must be explicitly excluded rather than silently ignored.

## Module checks

* [ ] One primary responsibility.
* [ ] Cohesive contents.
* [ ] Avoid giant `utils`.
* [ ] Avoid giant `helpers`.
* [ ] Avoid giant `common`.
* [ ] Public API significantly smaller than internal implementation where possible.

---

# 32. Function and Method Size

* [ ] Functions perform one conceptual operation.
* [ ] Prefer functions below **40 logical lines**.
* [ ] Review functions above **50 logical lines**.
* [ ] Strongly refactor above **80 logical lines**.
* [ ] Functions above **100 logical lines** require explicit justification.
* [ ] Do not create meaningless micro-functions only to satisfy metrics.
* [ ] Function extraction must create meaningful concepts.
* [ ] Validation, transformation, persistence, transport, and orchestration are separated where doing so improves clarity.

---

# 33. Cyclomatic Complexity

## Default project limit

* [ ] **Cyclomatic complexity ≤ 10 per function.**

## Interpretation

* [ ] `1–4`: low.
* [ ] `5–7`: moderate.
* [ ] `8–10`: review carefully.
* [ ] `11–15`: refactoring expected.
* [ ] `>15`: fail quality gate unless explicitly justified.
* [ ] `>20`: prohibited in normal production code.

Reduce complexity using:

* [ ] Guard clauses.
* [ ] Smaller responsibilities.
* [ ] Named predicates.
* [ ] Tables/maps instead of condition forests.
* [ ] Extracted state transitions.
* [ ] Better domain types.
* [ ] Eliminated duplicate branches.

Do not cheat complexity metrics by moving incomprehensible fragments into meaningless helpers.

---

# 34. Cognitive Complexity and Nesting

* [ ] Prefer maximum nesting depth **3**.
* [ ] Depth above **3** triggers review.
* [ ] Deep `if` trees are refactored.
* [ ] Deep loop nesting is reviewed.
* [ ] Boolean forests are decomposed.
* [ ] Cognitive complexity is measured where tooling supports it.
* [ ] High cognitive complexity is considered a readability defect even when cyclomatic complexity remains acceptable.
* [ ] Control flow should be understandable without mentally simulating many states simultaneously.

---

# 35. Duplication

* [ ] Detect duplicate code automatically.
* [ ] No copy-pasted business rules.
* [ ] No duplicated validation logic.
* [ ] No duplicated protocol contracts.
* [ ] No duplicated constants.
* [ ] No duplicated SQL logic where consolidation clearly improves maintenance.
* [ ] New substantial duplicated blocks trigger review.
* [ ] Three similar implementations trigger an abstraction review.
* [ ] Intentional duplication may remain when abstraction would increase coupling.

## Suggested gate

* [ ] New-code duplication target: **≤ 3%**.
* [ ] Whole-project duplication target: **≤ 5%**.
* [ ] New duplicated block around **10+ meaningful lines** triggers review.

Important principle:

> Duplication is often cheaper than the wrong abstraction.

---

# 36. No Else Rule

## Default rule

* [ ] Prefer **zero `else` branches** in normal imperative control flow.
* [ ] Handle invalid conditions first.
* [ ] Return early.
* [ ] Continue early.
* [ ] Break early.
* [ ] Propagate errors early.
* [ ] Keep the happy path unindented.
* [ ] Avoid `else if` chains where guards or explicit state dispatch are clearer.

Prefer:

```rust
if !valid {
    return Err(Error::Invalid);
}

process()
```

instead of:

```rust
if valid {
    process()
} else {
    Err(Error::Invalid)
}
```

Prefer:

```ts
if (!user) {
  return null;
}

return renderUser(user);
```

instead of:

```ts
if (user) {
  return renderUser(user);
} else {
  return null;
}
```

Allowed exceptions:

* [ ] Exhaustive value-producing expressions.
* [ ] Small symmetric alternatives.
* [ ] Language constructs where removing the branch decreases readability.
* [ ] Branches whose removal introduces duplicate execution.
* [ ] Explicit state-machine logic where exhaustive branching is the clearest representation.

Any substantial `else` should survive review intentionally.

---

# 37. Coupling

* [ ] Modules know only what they need.
* [ ] Dependencies use intentional interfaces.
* [ ] Domain code does not unnecessarily depend on infrastructure.
* [ ] UI does not access persistence directly without architecture justification.
* [ ] Persistence does not depend on UI.
* [ ] Feature internals are not accessed by unrelated features.
* [ ] Bidirectional dependencies are avoided.
* [ ] Circular dependencies = **0**.
* [ ] Fan-out is monitored.
* [ ] High fan-in abstractions remain stable.
* [ ] Changes do not cascade unnecessarily.

---

# 38. Cohesion

* [ ] Module contents belong together.
* [ ] Struct/class fields serve related responsibilities.
* [ ] Methods operate on related state.
* [ ] Feature code is not unnecessarily scattered.
* [ ] Data and behavior stay appropriately close.
* [ ] Utility modules do not mix unrelated concerns.
* [ ] Low-cohesion modules are split by actual responsibility.
* [ ] High-cohesion code is not fragmented merely to lower line counts.

---

# 39. Violations and Technical Debt

## New code must introduce zero

* [ ] Compiler warnings.
* [ ] Linter warnings.
* [ ] Formatter violations.
* [ ] Type errors.
* [ ] Failing tests.
* [ ] Critical security findings.
* [ ] High security findings unless formally accepted.
* [ ] Dead imports.
* [ ] Unreachable code.
* [ ] Accidentally committed secrets.
* [ ] Unjustified disabled tests.
* [ ] Unexplained lint suppressions.

## Suppressions

Review:

```rust
#[allow(...)]
```

```ts
// eslint-disable
```

```ts
// @ts-ignore
```

```ts
// @ts-expect-error
```

```python
# noqa
```

* [ ] Suppression has a concrete reason.
* [ ] Scope is minimal.
* [ ] Project-wide suppression is avoided.
* [ ] Suppression is removed when obsolete.
* [ ] New suppressions are visible during review.

## Technical debt

* [ ] Debt has a reason.
* [ ] Debt has impact.
* [ ] Debt has a removal condition where practical.
* [ ] TODOs are actionable.
* [ ] FIXMEs correspond to actual defects/risks.
* [ ] Obsolete TODO/FIXME items are removed.

---

# 40. Type Quality

* [ ] Use the strongest practical type.
* [ ] Avoid stringly typed APIs.
* [ ] Avoid primitive obsession for important domain concepts.
* [ ] Avoid boolean blindness.
* [ ] Distinct entity IDs cannot be mixed accidentally where practical.
* [ ] Units cannot be mixed accidentally.
* [ ] Invalid states are excluded through types where justified.
* [ ] Optional values represent genuine optionality.
* [ ] Nullable values are minimized.
* [ ] Closed sets use exhaustive variants/enums where appropriate.
* [ ] Public boundaries expose explicit types.
* [ ] Serialization types are separated from domain types when their responsibilities differ.
* [ ] Generic types improve safety rather than merely increasing abstraction.

---

# 41. Boolean and Conditional Complexity

* [ ] Avoid functions with many boolean flags.
* [ ] Avoid ambiguous boolean arguments.
* [ ] Replace complicated conditions with named predicates.
* [ ] Avoid conditions combining unrelated decisions.
* [ ] Prefer positive predicates.
* [ ] Avoid double negatives.
* [ ] Parenthesize mixed expressions clearly.
* [ ] Extract domain rules when a condition cannot be explained simply.
* [ ] Review conditions with more than approximately 3 independent boolean decisions.
* [ ] Prefer exhaustive state representation over boolean combinations where appropriate.

---

# 42. Parameters

* [ ] Prefer few parameters.
* [ ] Review functions with more than **4 parameters**.
* [ ] More than **6 parameters** normally requires restructuring.
* [ ] Related parameters are grouped into meaningful domain/configuration types.
* [ ] Avoid long runs of same-typed primitive parameters.
* [ ] Avoid boolean flag parameters when separate operations are clearer.
* [ ] Required and optional parameters are distinguishable.
* [ ] Builder pattern is used only when construction complexity actually warrants it.

---

# 43. State Management

* [ ] State has one authoritative owner.
* [ ] Duplicate state is minimized.
* [ ] Derived state is computed where practical.
* [ ] State transitions are explicit.
* [ ] Invalid transitions are prevented.
* [ ] Mutation scope is minimized.
* [ ] Global mutable state is avoided.
* [ ] Concurrent state has documented ownership.
* [ ] Persistent state has explicit schema.
* [ ] Temporary state has explicit lifetime.
* [ ] State synchronization is not manually duplicated across layers.

---

# 44. API Surface

* [ ] Default to private/internal.
* [ ] Public symbols have a reason to be public.
* [ ] Expose the minimum necessary API.
* [ ] Public APIs remain stable where required.
* [ ] Testing does not force internal helpers to become public unnecessarily.
* [ ] Naming is consistent across similar operations.
* [ ] Incorrect usage is difficult.
* [ ] Implementation details do not leak.
* [ ] Breaking changes are intentional and documented.
* [ ] Deprecated APIs have removal plans.

---

# 45. Data Flow

A developer should be able to answer:

* [ ] Where did this value originate?
* [ ] Where was it validated?
* [ ] Who owns it?
* [ ] Who may mutate it?
* [ ] Where does it leave the system?

Additionally:

* [ ] Input entry points are obvious.
* [ ] Transformations are explicit.
* [ ] Side effects occur in identifiable locations.
* [ ] Equivalent representations are not repeatedly converted.
* [ ] Domain values remain domain values internally.
* [ ] Sensitive data flow can be traced.
* [ ] External representations are normalized once.

---

# 46. Resource Management

* [ ] Files close deterministically.
* [ ] Locks release deterministically.
* [ ] Transactions finalize deterministically.
* [ ] Temporary files are removed.
* [ ] Child processes are reaped.
* [ ] Connections are bounded.
* [ ] Subscriptions/listeners unsubscribe.
* [ ] Timers are cancelled.
* [ ] Background tasks have explicit owners.
* [ ] UI effects clean up.
* [ ] Cleanup works on success paths.
* [ ] Cleanup works on error paths.
* [ ] RAII/lifetime ownership is used where the language supports it.

---

# 47. Determinism

* [ ] Tests produce repeatable results.
* [ ] Builds are reproducible where practical.
* [ ] Externally visible sorting is deterministic.
* [ ] Generated output is stable.
* [ ] Time dependence is isolated.
* [ ] Randomness is seedable in tests.
* [ ] Concurrency avoids unnecessary nondeterminism.
* [ ] Environment-dependent behavior is explicit.
* [ ] Snapshot/golden results do not change spuriously.

---

# 48. Boundaries

At every:

* HTTP boundary
* IPC boundary
* CLI boundary
* database boundary
* filesystem boundary
* environment-variable boundary
* configuration boundary
* plugin boundary
* third-party API boundary
* message-queue boundary
* user-input boundary

verify:

* [ ] Input validation.
* [ ] Explicit types.
* [ ] Size limits.
* [ ] Missing-field behavior.
* [ ] Unknown-field behavior.
* [ ] Unknown-enum behavior.
* [ ] Encoding/decoding failures.
* [ ] External values converted into trusted internal types.
* [ ] External assumptions never silently become internal invariants.

---

# 49. Refactoring and Complexity Budget

Before refactoring:

* [ ] Existing tests pass.
* [ ] Existing behavior is understood.
* [ ] Regression tests cover critical behavior.
* [ ] Performance baseline exists where relevant.

During refactoring:

* [ ] Structural changes are separated from intentional behavior changes where practical.
* [ ] Complexity decreases rather than moves.
* [ ] Public behavior remains stable.
* [ ] Commits remain understandable.

Every new item should justify its complexity cost:

* [ ] Dependency.
* [ ] Abstraction.
* [ ] Trait/interface.
* [ ] Generic parameter.
* [ ] Global state.
* [ ] Worker.
* [ ] Queue.
* [ ] Cache.
* [ ] Protocol.
* [ ] Configuration option.
* [ ] Feature flag.
* [ ] Database table.
* [ ] Service.
* [ ] Architectural layer.

Ask:

> What complexity can be removed to compensate for the complexity being added?

---

# 50. Final Quality Gate / Definition of Done

A change is complete only when all applicable gates pass.

## Correctness

* [ ] Required behavior works.
* [ ] Existing behavior remains intact.
* [ ] Main workflow is validated.
* [ ] Failure behavior is validated.

## Static quality

* [ ] Compiler warnings = **0**.
* [ ] Linter warnings = **0**.
* [ ] Type errors = **0**.
* [ ] Formatting violations = **0**.
* [ ] Circular dependencies = **0**.
* [ ] Unexplained suppressions = **0**.
* [ ] Unused dependencies = **0**.
* [ ] Dead imports = **0**.
* [ ] Accidental dead code = **0**.

## Structural quality

* [ ] Hand-written files ≤ **1000 lines**.
* [ ] Files > **500 lines** reviewed.
* [ ] Functions > **50 logical lines** reviewed.
* [ ] Cyclomatic complexity target ≤ **10**.
* [ ] Maximum preferred nesting ≤ **3**.
* [ ] More than **4 parameters** reviewed.
* [ ] New-code duplication ≤ **3%**.
* [ ] No unjustified `else`.
* [ ] No god modules/classes.

## Testing

* [ ] Unit tests pass.
* [ ] Integration tests pass.
* [ ] Relevant E2E tests pass.
* [ ] Bug fixes have regression tests.
* [ ] Coverage does not materially regress.
* [ ] Critical changed code is exercised.

## Security

* [ ] Critical vulnerabilities = **0**.
* [ ] High vulnerabilities = **0**, unless deliberately accepted.
* [ ] No secrets committed.
* [ ] Dependency scan passes.
* [ ] Static security scan passes.

## Build

* [ ] Debug build passes.
* [ ] Release build passes.
* [ ] Supported-platform builds pass where applicable.

## Performance

* [ ] Performance-sensitive changes have measurements.
* [ ] No unexplained material regression.

## Documentation

* [ ] Documentation updated.
* [ ] Configuration documented.
* [ ] Changelog updated when appropriate.

## Final diff

* [ ] Contains only intended changes.
* [ ] Temporary debugging removed.
* [ ] Temporary logging removed.
* [ ] Obsolete implementation removed.
* [ ] CI green.

The required outcome is:

> **Correct + Simple + Readable + Reliable + Testable + Maintainable + Efficient.**

---

# Researched Quality-Gate Tooling

The best approach is **not** to search for one universal library that does everything. Use a small set of cross-language gates plus native tooling for each ecosystem.

`jscpd` is particularly useful here because its current implementation is Rust-based and advertises duplicate detection across **223 formats**, making it appropriate as a repository-wide duplication gate rather than merely a JavaScript tool. ([jscpd][1])

SonarQube can centrally gate metrics including coverage, duplicated code, and complexity, and its quality-gate model is designed specifically for accepting or rejecting code according to configured conditions. ([SonarSource Docs][2])

MegaLinter is another useful umbrella for polyglot repositories; its current documentation advertises support for **63 languages** plus additional formats/tooling formats and integrates many native linters rather than replacing them. ([MegaLinter][3])

GitHub CodeQL currently supports C/C++, C#, Go, Java/Kotlin, JavaScript/TypeScript, Python, Ruby, Rust, Swift, and GitHub Actions workflows, making it a strong second security-analysis layer for repositories using those languages. ([GitHub Docs][4])

---

## Cross-Language Gates

### jscpd

Use for:

* duplication
* copy/paste detection
* repository-wide duplicate blocks
* polyglot monorepos

Recommended gate:

```text
new-code duplication <= 3%
whole-project duplication <= 5%
minimum meaningful duplicated block ~= 10 lines
```

This should run independently of the normal language linter.

### SonarQube / SonarQube Cloud

Use for:

* quality gates
* code smells
* maintainability
* coverage aggregation
* duplication
* cyclomatic/cognitive complexity
* security findings
* technical-debt trends

Recommended:

```text
new code:
  critical issues = 0
  high issues = 0
  coverage >= 80%
  duplication <= 3%

whole project:
  duplication <= 5%
  quality gate = PASS
```

Sonar's own documentation exposes coverage, duplication, and complexity as gateable metrics. ([SonarSource Docs][2])

### CodeQL

Use as a security/correctness gate where supported.

Recommended:

```text
new critical alerts = 0
new high alerts = 0
```

GitHub also supports merge protection based on code-scanning results. ([GitHub Docs][5])

### MegaLinter

Best for:

* polyglot monorepos
* configuration files
* Dockerfiles
* Markdown
* YAML
* JSON
* Terraform
* shell
* uncommon languages
* centrally orchestrating many ecosystem linters

It should generally orchestrate native tools rather than replace direct language-specific configuration. ([MegaLinter][6])

---

# Rust

Recommended stack:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --release               # where valuable
cargo audit
cargo deny check
cargo llvm-cov                     # coverage
jscpd                              # duplication
rust-code-analysis                 # complexity/metrics
```

Clippy is distributed with the Rust toolchain and exists specifically to catch common mistakes and improve Rust code. ([Rust Documentation][7])

`cargo-deny` provides dependency-oriented policy checking, while RustSec maintains `cargo-audit` for advisory-based auditing. ([GitHub][8])

For your complexity rule, `rust-code-analysis` calculates cyclomatic complexity, cognitive complexity, source-line metrics, and several additional structural metrics. ([GitHub][9])

Recommended Rust gates:

```text
fmt violations        = 0
clippy warnings       = 0
unsafe dependency CVEs= 0 critical/high
cyclomatic complexity <= 10
file lines            <= 1000
duplication new code  <= 3%
```

For **No Else**, use Clippy where an equivalent lint applies, plus project review/custom AST checks for stricter enforcement. A blanket ban on every `else` is more reliably implemented as a project-specific source/AST rule than by depending on Clippy alone.

---

# TypeScript / JavaScript

Recommended stack:

```text
tsc --noEmit
ESLint + typescript-eslint
Biome                    # optional formatter/linter consolidation
Knip
dependency-cruiser
jscpd
Vitest/Jest coverage
CodeQL/Semgrep
```

ESLint natively provides rules covering cyclomatic complexity, maximum lines, maximum lines per function, maximum nesting depth, maximum parameters, and `no-else-return`. ([ESLint][10])

`typescript-eslint` provides type-aware linting and strict type-checked configurations, which catch issues that syntax-only linting cannot. ([TypeScript ESLint][11])

Knip detects unused files, dependencies, exports, types, unresolved imports, duplicate exports, and optionally dependency cycles. ([Knip][12])

`dependency-cruiser` can enforce architectural dependency rules and explicitly reject circular dependencies. ([GitHub][13])

Recommended ESLint gates:

```js
{
  complexity: ["error", 10],
  "max-depth": ["error", 3],
  "max-lines": ["error", {
    max: 1000,
    skipBlankLines: true,
    skipComments: true
  }],
  "max-lines-per-function": ["error", {
    max: 50,
    skipBlankLines: true,
    skipComments: true
  }],
  "max-params": ["error", 4],
  "no-else-return": ["error", { allowElseIf: false }]
}
```

Recommended additional gates:

```text
tsc errors                    = 0
eslint warnings/errors         = 0
Knip unused dependencies       = 0
Knip unused files              = 0
runtime circular dependencies  = 0
jscpd new duplication          <= 3%
```

Biome is now a substantial JS/TS lint/format toolchain and can replace portions of ESLint/formatter setups when its rule coverage is sufficient for the project. ([Biome][14])

---

# Python

Recommended stack:

```text
ruff format --check
ruff check
mypy or pyright
pytest
pytest-cov
Bandit
pip-audit
jscpd
Radon / Xenon
```

Ruff contains a McCabe-complexity `C901` rule and its default configurable maximum is `10`, which aligns directly with this checklist. ([Astral Docs][15])

Radon calculates cyclomatic complexity, raw line metrics, Halstead metrics, and maintainability index. ([Radon][16])

Pylint additionally exposes structural checks such as too many branches, statements, nested blocks, locals, arguments, and module lines. ([Pylint][17])

Bandit performs AST-based security checks, while `pip-audit` scans Python environments for packages with known vulnerabilities. ([Bandit][18])

Recommended:

```text
Ruff errors                 = 0
type checker errors         = 0
McCabe complexity           <= 10
module lines                <= 1000
test failures               = 0
Bandit high severity        = 0
pip-audit critical/high     = 0
duplication new code        <= 3%
```

---

# Go

Recommended stack:

```text
gofmt
go vet
go test ./...
go test -race ./...
golangci-lint
govulncheck
jscpd
```

Current `golangci-lint` includes dedicated analyzers for `cyclop`, `gocyclo`, `gocognit`, `dupl`, `funlen`, `nestif`, `maintidx`, `govet`, `gosec`, `staticcheck`, unused code, and many other checks. ([GolangCI-Lint][19])

Its configuration supports a cyclomatic threshold of `10`, duplicate-fragment thresholds, function-length limits, and nesting analysis directly. ([GolangCI-Lint][20])

`govulncheck` checks Go programs against known vulnerabilities using the relevant build configuration. ([Go Packages][21])

Recommended:

```text
cyclop max-complexity = 10
funlen lines          = 50
nesting               <= 3
dupl                   enabled
gosec findings high   = 0
govulncheck reachable critical/high = 0
race detector failures = 0
```

---

# Java

Recommended stack:

```text
Checkstyle
PMD
PMD CPD
SpotBugs
Error Prone
JaCoCo
ArchUnit
OWASP dependency-check or equivalent
CodeQL
```

Checkstyle has a `FileLength` check whose maximum is configurable, so your **1000-line hard limit** can be enforced directly. ([Checkstyle][22])

PMD provides cyclomatic and cognitive complexity checks; its Java cyclomatic-complexity rule defaults to reporting methods at complexity `10`. ([PMD][23])

PMD's Copy/Paste Detector is a natural Java-native duplication companion, although `jscpd` can provide a single duplication policy across a multi-language repository.

SpotBugs analyzes compiled Java bytecode for correctness, performance, security, multithreading, and related bug classes. ([SpotBugs][24])

JaCoCo remains the standard open-source Java coverage library. ([JaCoCo][25])

Recommended:

```text
Checkstyle file max     = 1000
PMD cyclomatic max      = 10
PMD cognitive max       = 15
CPD/jscpd duplication   <= 3% new code
SpotBugs high findings  = 0
JaCoCo changed coverage >= 80%
```

---

# Kotlin

Recommended stack:

```text
detekt
ktlint
Kover
JUnit
ArchUnit
CodeQL
jscpd
```

Detekt is a dedicated Kotlin static-analysis system and supports integration with ktlint formatting rules. ([GitHub][26])

Recommended:

```text
detekt findings       = 0
complexity            <= 10
nesting               <= 3
file length           <= 1000
ktlint violations     = 0
duplication new code  <= 3%
coverage              >= 80%
```

For Android projects also keep Android Lint enabled.

---

# C / C++

Recommended stack:

```text
clang-format
clang-tidy
Clang Static Analyzer
Cppcheck
Include-What-You-Use
CodeChecker
Lizard
gcov / llvm-cov
jscpd
CodeQL
```

`clang-tidy` is an extensible Clang-based C++ linter/static-analysis framework for diagnosing bugs, interface misuse, style violations, and other problems. ([Clang][27])

CodeChecker can orchestrate Clang-Tidy, Clang Static Analyzer, Cppcheck, GCC Static Analyzer, and other analyzers. ([CodeChecker][28])

Recommended:

```text
compiler warnings       = 0
-Werror                 enabled in CI where practical
clang-tidy findings     = 0
complexity              <= 10
file lines              <= 1000
duplication new code    <= 3%
sanitizer failures      = 0
```

Also run where applicable:

```text
ASan
UBSan
TSan
MSan
```

These runtime tools complement rather than replace static quality gates.

---

# C# / .NET

Recommended stack:

```text
dotnet format --verify-no-changes
Roslyn .NET analyzers
StyleCop.Analyzers
SonarAnalyzer.CSharp
dotnet test
coverlet
CodeQL
jscpd
```

Roslyn analyzers are included with modern .NET SDKs and analyze C#/VB code for quality and style issues. ([Microsoft Learn][29])

`dotnet format --verify-no-changes` provides a native CI formatting gate. ([Microsoft Learn][30])

Recommended:

```text
warnings                = errors
format violations       = 0
analyzer findings       = 0
file lines              <= 1000
complexity              <= 10
duplication new code    <= 3%
coverage                >= 80%
```

Use SonarAnalyzer or custom Roslyn analyzers for project-specific structural rules such as strict No Else or exact complexity/file constraints not covered by the chosen built-in rules.

---

# PHP

Recommended stack:

```text
PHPStan
Psalm
PHP-CS-Fixer
PHPMD
PHPUnit
Infection
jscpd
Composer audit
```

PHPStan currently exposes analysis levels from `0` through `10` and supports custom project rules. ([PHPStan][31])

PHPMD includes cyclomatic complexity, NPath complexity, excessive method length, excessive class length, excessive parameter lists, and related size/maintainability rules. ([PHPMD][32])

Recommended:

```text
PHPStan level          = 10 where practical
PHPMD cyclomatic       <= 10
file lines             <= 1000
method lines           <= 50
duplication new code   <= 3%
PHPUnit failures       = 0
mutation score         tracked for critical logic
```

---

# Ruby / Rails

Recommended stack:

```text
RuboCop
Brakeman
RSpec/Minitest
SimpleCov
jscpd
bundler-audit
```

RuboCop's Metrics cops directly measure class length, method length, and cyclomatic complexity; its current default `Metrics/CyclomaticComplexity` maximum is `7`, stricter than the proposed universal limit of `10`. ([RuboCop Documentation][33])

Brakeman provides Rails-specific source-based security analysis. ([Brakeman Scanner][34])

Recommended:

```text
RuboCop offenses       = 0
cyclomatic complexity  <= 10
method lines           <= 50
file lines             <= 1000
Brakeman high findings = 0
duplication new code   <= 3%
```

---

# Swift

Recommended stack:

```text
swift-format / SwiftFormat
SwiftLint
swift test / XCTest
CodeQL
jscpd
```

SwiftLint has dedicated rules including `file_length`, `function_body_length`, `function_parameter_count`, and cyclomatic-complexity-related checks. ([Realm][35])

Recommended:

```text
file_length warning       = 500
file_length error         = 1000
function_body_length      = 50
cyclomatic_complexity     = 10
function_parameter_count  = 4
SwiftLint warnings        = 0
duplication new code      <= 3%
```

---

# Dart / Flutter

Recommended stack:

```text
dart format
dart analyze --fatal-infos
flutter analyze
flutter test
flutter test --coverage
flutter_lints
jscpd
```

`dart analyze` performs command-line static analysis and can be configured so informational diagnostics fail the process using `--fatal-infos`. ([Dart][36])

Flutter's recommended `flutter_lints` package provides the standard recommended lint baseline for Flutter projects. ([Flutter Documentation][37])

Recommended:

```text
dart analyze issues       = 0
format violations         = 0
file lines                <= 1000
complexity                <= 10
duplication new code      <= 3%
tests                     = PASS
```

For complexity and structural limits not exposed directly by the standard analyzer, add a dedicated metrics analyzer or enforce them through SonarQube/custom analysis.

---

# Shell / Bash

Recommended stack:

```text
shellcheck
shfmt
bats
jscpd
```

Recommended:

```text
ShellCheck warnings    = 0
shfmt diff             = 0
file lines             <= 1000
function complexity    manually/tool-gated where relevant
duplication            <= 3%
```

Shell scripts approaching hundreds of lines should usually trigger consideration of whether the logic belongs in a general-purpose language.

---

# SQL

Recommended stack:

```text
SQLFluff
database-native EXPLAIN tooling
migration tests
jscpd
SonarQube where supported
```

Gate:

```text
lint findings           = 0
unsafe unparameterized application SQL = 0
missing migration verification = 0
known N+1 patterns      = 0
unreviewed hot-path full scans = 0
```

SQL quality requires query-plan validation in addition to lexical linting.

---

# Terraform / HCL

Recommended stack:

```text
terraform fmt -check
terraform validate
TFLint
Checkov or Trivy config scanning
terraform test where applicable
MegaLinter
```

Gate:

```text
format violations  = 0
validation errors   = 0
TFLint findings     = 0
critical misconfigs = 0
high misconfigs     = 0
```

---

# YAML / JSON / TOML / Configuration

Recommended:

```text
Prettier / dprint / Biome where supported
yamllint
JSON Schema validation
Taplo for TOML
MegaLinter
```

Gate:

```text
syntax errors         = 0
schema errors         = 0
format violations     = 0
duplicate keys        = 0
unknown config fields = 0 where schemas allow validation
```

---

# Less Common Languages

For Scala, Lua, Elixir, Erlang, Haskell, OCaml, Zig, Clojure, Groovy, PowerShell, Perl, R, Solidity, Apex, and other ecosystems, the same policy applies:

```text
native formatter
+ native compiler/type checker
+ strongest maintained ecosystem linter
+ native test framework
+ native coverage tool
+ dependency/security scanner where available
+ jscpd for duplication
+ SonarQube where supported
+ MegaLinter for repository-level orchestration
```

MegaLinter's current language coverage makes it particularly useful for this long tail of languages and file formats. ([MegaLinter][3])

---

# Recommended Universal Quality Pipeline

For a polyglot project, I would standardize CI into these layers:

```text
01. lockfile/dependency integrity
02. formatter
03. compiler/type checker
04. language-native linter
05. architecture/dependency-cycle checker
06. dead-code/unused-dependency checker
07. file-size gate
08. function-size gate
09. cyclomatic-complexity gate
10. nesting/cognitive-complexity gate
11. No Else / guard-clause policy
12. jscpd duplication gate
13. unit tests
14. integration tests
15. E2E tests
16. coverage gate
17. dependency vulnerability scan
18. SAST / CodeQL / Semgrep
19. release build
20. SonarQube or equivalent aggregate quality gate
```

Recommended universal defaults:

```text
compiler warnings                     = 0
linter warnings                       = 0
type errors                           = 0
formatter violations                  = 0
failing tests                         = 0
circular dependencies                 = 0

critical security findings            = 0
high security findings                = 0

maximum handwritten source file       = 1000 lines
file review threshold                 = 500 lines

preferred function size               <= 40 logical lines
function review threshold             = 50 logical lines
strong function refactor threshold    = 80 logical lines

cyclomatic complexity                 <= 10
cognitive complexity                  <= 15
preferred nesting depth               <= 3

parameter review threshold            > 4
strong parameter refactor threshold   > 6

new-code duplication                  <= 3%
whole-project duplication             <= 5%

new unexplained lint suppressions     = 0
unused dependencies                   = 0
dead imports                          = 0
accidental dead code                  = 0

changed-code coverage                 >= 80%
```

---

# Best Tooling Combination for Rust + TypeScript Desktop/Backend Projects

For the project profile this checklist was originally designed around, I would use:

```text
Repository-wide
├── jscpd
├── SonarQube
├── CodeQL
├── dependency vulnerability scanning
└── optional MegaLinter

Rust
├── rustfmt
├── clippy -D warnings
├── cargo test
├── cargo llvm-cov
├── cargo audit
├── cargo deny
└── rust-code-analysis

TypeScript
├── tsc --noEmit
├── ESLint
├── typescript-eslint strict-type-checked
├── Knip
├── dependency-cruiser
├── Vitest/Jest
└── coverage

Frontend
├── accessibility tests
├── component tests
└── Playwright E2E

Desktop/Tauri
├── Rust integration tests
├── typed IPC contract tests
├── frontend E2E
└── release-build smoke tests
```

The TypeScript side is particularly enforceable: ESLint already exposes native rules for complexity, file length, function length, nesting depth, parameter count, and `no-else-return`, while Knip and dependency-cruiser cover dead code/dependencies and architecture cycles respectively. ([ESLint][38])

On the Rust side, Clippy should remain the primary correctness/idiom linter, with `rust-code-analysis` supplying the structural complexity metrics Clippy does not provide as a complete quality-gate system. ([Rust Documentation][7])

---

# Final Quality-Gate Policy

The tools should **enforce the checklist**, not become the checklist.

A project should fail CI when:

```text
behavior breaks
OR tests fail
OR compiler/type errors exist
OR warnings exist
OR format differs
OR critical/high security findings exist
OR dependency policy fails
OR circular dependencies appear
OR a handwritten file exceeds 1000 lines
OR cyclomatic complexity exceeds the accepted threshold
OR substantial new duplication exceeds the accepted threshold
OR required coverage falls below the accepted threshold
OR required release builds fail
```

Metrics should not be gamed.

Do not:

```text
split one bad 1200-line module into two arbitrary 600-line modules
extract meaningless functions merely to reduce complexity
introduce abstractions solely to eliminate duplication
silence lint rules solely to make CI green
exclude difficult files from analysis
write low-value tests solely to increase coverage
```

Instead:

> **Use quality gates to identify design pressure, then improve the design.**

The strongest practical combination for a modern polyglot project is:

> **native compiler/type checker + native linter + native tests + jscpd + dependency/architecture analysis + security scanning + an aggregate quality gate such as SonarQube.**

That provides measurable enforcement for **Code Quality, Efficiency, Readability, Reliability, Testability, Maintainability, Dependencies, Module Size, Cyclomatic Complexity, Cognitive Complexity, Duplication, Violations, the 1000-line maximum, and the No Else policy** without forcing one tool to solve every problem.

[1]: https://jscpd.dev/?utm_source=chatgpt.com "jscpd - Copy/Paste Detector for Source Code - jscpd"
[2]: https://docs.sonarsource.com/sonarqube-server/quality-standards-administration/managing-quality-gates/introduction-to-quality-gates?utm_source=chatgpt.com "Understanding quality gates | SonarQube Server"
[3]: https://megalinter.io/?utm_source=chatgpt.com "MegaLinter by OX Security"
[4]: https://docs.github.com/code-security/code-scanning/introduction-to-code-scanning/about-code-scanning-with-codeql?utm_source=chatgpt.com "Code scanning with CodeQL"
[5]: https://docs.github.com/en/code-security/reference/code-scanning/workflow-configuration-options?utm_source=chatgpt.com "Workflow configuration options for code scanning"
[6]: https://megalinter.io/8/supported-linters/?utm_source=chatgpt.com "List of the 100+ supported linters embedded ..."
[7]: https://doc.rust-lang.org/cargo/commands/cargo-clippy.html?utm_source=chatgpt.com "cargo clippy - The Cargo Book"
[8]: https://github.com/embarkstudios/cargo-deny?utm_source=chatgpt.com "EmbarkStudios/cargo-deny: ❌ Cargo plugin for linting your ..."
[9]: https://github.com/mozilla/rust-code-analysis?utm_source=chatgpt.com "mozilla/rust-code-analysis: Library to analyze and ..."
[10]: https://eslint.org/docs/latest/rules/?utm_source=chatgpt.com "Rules Reference - ESLint - Pluggable JavaScript Linter"
[11]: https://typescript-eslint.io/users/configs/?utm_source=chatgpt.com "Shared Configs"
[12]: https://knip.dev/?utm_source=chatgpt.com "Knip: Declutter your JavaScript & TypeScript projects"
[13]: https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md?utm_source=chatgpt.com "dependency-cruiser/doc/rules-reference.md at main"
[14]: https://biomejs.dev/?utm_source=chatgpt.com "Biome, toolchain of the web"
[15]: https://docs.astral.sh/ruff/rules/complex-structure/?utm_source=chatgpt.com "complex-structure (C901) | Ruff - Astral Docs"
[16]: https://radon.readthedocs.io/en/latest/?utm_source=chatgpt.com "Welcome to Radon's documentation!"
[17]: https://pylint.readthedocs.io/en/latest/messages/refactor/too-many-branches.html?utm_source=chatgpt.com "too-many-branches / R0912 - Pylint 4.1.0-dev0 documentation"
[18]: https://bandit.readthedocs.io/?utm_source=chatgpt.com "Welcome to Bandit — Bandit documentation"
[19]: https://golangci-lint.run/docs/linters/?utm_source=chatgpt.com "Linters – Golangci-lint"
[20]: https://golangci-lint.run/docs/linters/configuration/?utm_source=chatgpt.com "Settings – Golangci-lint"
[21]: https://pkg.go.dev/golang.org/x/vuln/cmd/govulncheck?utm_source=chatgpt.com "govulncheck command - golang.org/x/vuln/cmd ..."
[22]: https://checkstyle.org/checks/sizes/filelength.html?utm_source=chatgpt.com "FileLength – checkstyle"
[23]: https://pmd.github.io/pmd/pmd_rules_java_design.html?utm_source=chatgpt.com "Design | PMD Source Code Analyzer"
[24]: https://spotbugs.readthedocs.io/en/stable/?utm_source=chatgpt.com "SpotBugs manual — spotbugs 4.10.3 documentation"
[25]: https://www.jacoco.org/jacoco/?utm_source=chatgpt.com "JaCoCo Java Code Coverage Library"
[26]: https://github.com/detekt/detekt?utm_source=chatgpt.com "Detekt - Static code analysis for Kotlin · GitHub"
[27]: https://clang.llvm.org/extra/clang-tidy/?utm_source=chatgpt.com "Clang-Tidy — Extra Clang Tools 24.0.0git documentation"
[28]: https://codechecker.readthedocs.io/?utm_source=chatgpt.com "CodeChecker - Read the Docs"
[29]: https://learn.microsoft.com/en-us/dotnet/fundamentals/code-analysis/overview?utm_source=chatgpt.com "Code analysis in .NET"
[30]: https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-format?utm_source=chatgpt.com "dotnet format command - .NET CLI | Microsoft Learn"
[31]: https://phpstan.org/documentation?utm_source=chatgpt.com "Documentation | PHPStan"
[32]: https://phpmd.org/rules/codesize.html?utm_source=chatgpt.com "PHPMD Code Size Rules"
[33]: https://docs.rubocop.org/rubocop/latest/cops_metrics.html?utm_source=chatgpt.com "Metrics :: RuboCop Docs"
[34]: https://brakemanscanner.org/docs/?utm_source=chatgpt.com "Brakeman: Documentation"
[35]: https://realm.github.io/SwiftLint/swift-syntax-dashboard.html?utm_source=chatgpt.com "Swift Syntax Dashboard Reference"
[36]: https://dart.dev/tools/dart-analyze?utm_source=chatgpt.com "dart analyze"
[37]: https://docs.flutter.dev/release/breaking-changes/flutter-lints-package?utm_source=chatgpt.com "Introducing package:flutter_lints"
[38]: https://zh-hans.eslint.org/docs/latest/rules/?utm_source=chatgpt.com "规则参考 - ESLint - 插件化的 JavaScript 代码检查工具"
