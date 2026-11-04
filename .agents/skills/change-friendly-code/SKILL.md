---
name: change-friendly-code
description: >-
  Use when writing, reviewing, or refactoring application code and the goal is code that is easy to understand, change, and trust — not merely code that runs. Applies seven senior-engineer laws: keep the main path flat with guard clauses, name by business meaning, keep external systems behind a translation boundary, make invalid states hard to represent, separate decisions from side effects, make errors machine-readable and safe to log, and keep changes narrowly scoped. Includes smell signals, before/after examples, TypeScript/Python/Go notes, a review audit, and verification checks. Complements code-simplification (behavior-preserving cleanup) and code-reviewer (diff review).
license: MIT
metadata:
  author: Emmraan
  version: 1.0.0
---

# Change-Friendly Code

Software does not become expensive because it is long. It becomes expensive because every change requires understanding more context than the last one. This skill is a lens for writing and reviewing code so the next change costs less than this one.

**Core principle: make the next change easier.**

Not shorter. Not cleverer. Not perfect. The test is whether the next engineer — often future you, with no memory of today — can change one behavior without touching five unrelated ones.

## When to Use

- Writing a feature that will be maintained, not thrown away.
- Reviewing a diff for maintainability, not only correctness.
- Refactoring code that is hard to change, test, or reason about.
- Integrating a third-party API, SDK, webhook, or legacy module.
- Adding validation, state, or error handling to an existing flow.
- Onboarding to unfamiliar code and deciding what to improve first.

## When Not to Use

- Throwaway scripts, one-off migrations, and spike code that will be deleted.
- The code is already clear and the change is genuinely local. Do not refactor for its own sake.
- The problem is architectural — service boundaries, scaling, data ownership: use `architecture-designer` first.
- The task is purely behavior-preserving cleanup of code you already understand: use `code-simplification`.
- The task needs a formal PR review with a structured report: use `code-reviewer`, and apply this skill as the maintainability lens.
- Team conventions conflict with a law. Follow the codebase; consistency beats personal preference.

## The Three Passes

Apply the laws at three moments, not once at the end.

| Pass | Question | Output |
|---|---|---|
| Before coding | What is the most likely next change to this area? | A shape that makes that change local |
| While coding | Does this line expose a decision, a boundary, or a state? | Guard clause, type, adapter, or pure function |
| Before review | Can a reviewer hold this in their head? | A focused diff and a one-sentence rationale |

The laws are diagnostic, not ceremonial. Use the ones the code needs.

## The Seven Laws

### Law 1 — Keep the Main Path Easy to Follow

**Smell:** the happy path is buried three `if`s deep; error branches occupy most of the function; the reader must simulate nesting to find the actual work.

**Move:** reject first, then write the main path at one indentation level. A guard clause answers "can we continue?" so the rest of the function can assume the answer.

```typescript
// Hard to follow: the real work is the last line of a pyramid.
async function publish(post: Post) {
  if (post) {
    if (post.authorId === currentUser.id) {
      if (post.status === 'draft') {
        if (post.body.trim().length > 0) {
          await repo.publish(post.id)
          return { ok: true }
        }
      }
    }
  }
  return { ok: false }
}
```

```typescript
// Main path at one level. Each rejection is explicit.
async function publish(post: Post | null) {
  if (!post) return fail('POST_NOT_FOUND')
  if (post.authorId !== currentUser.id) return fail('NOT_AUTHOR')
  if (post.status !== 'draft') return fail('ALREADY_PUBLISHED')
  if (post.body.trim().length === 0) return fail('EMPTY_BODY')

  await repo.publish(post.id)
  return { ok: true }
}
```

**Check:** can you find the operation that matters within five seconds? If not, flatten it.

**Tradeoff:** guard clauses multiply return points. That is fine when each return is a distinct outcome; it is not fine when a function has fifteen unrelated exits. Past that point, split the function.

### Law 2 — Name Things by Meaning

**Smell:** `data`, `result`, `item`, `info`, `handleThing`, `processPayload`; a boolean named `flag`; a variable whose type is its only description.

**Move:** name the business concept, not the container. A name should answer "what is this in the domain?" without requiring the reader to trace its assignment.

```typescript
// The reader must trace all three variables to learn what they hold.
const data = await api.get('/orders')
const result = data.filter((item) => item.s === 'p')
const list = result.map((item) => item.id)
```

```typescript
// The names carry the rule.
const orders = await api.get('/orders')
const unpaidOrders = orders.filter((order) => order.status === 'pending_payment')
const unpaidOrderIds = unpaidOrders.map((order) => order.id)
```

Rules:

- Name by role in the domain: `refundablePayments`, not `filteredData`.
- Name booleans as assertions: `isEligibleForRefund`, not `refundFlag`.
- Name functions by the outcome they produce: `approveRefund`, not `handleRefundLogic`.
- Avoid encoding types in names (`orderArray`, `userMap`) unless the type is the point.
- A comment that explains a name is a signal that the name is wrong.
- Do not make names longer for their own sake. `tax` beats `calculatedApplicableTaxAmount`.

**Check:** could a new teammate guess what this holds without reading the next ten lines?

**Tradeoff:** domain language can be unfamiliar to newcomers. That is why the domain glossary belongs in the repo, not in a reviewer's head.

### Law 3 — Keep External Systems Behind a Boundary

**Smell:** Stripe's `amount_total`, `customer`, and `payment_intent` fields used directly in business logic; SDK types in function signatures; provider-specific conditionals scattered across the codebase; a webhook payload used as the internal model.

**Move:** translate at the edge, once. The rest of the application speaks its own language and does not know who supplied the data.

```typescript
// External shape leaks inward. Every caller now depends on Stripe.
function isPaid(event: Stripe.Event) {
  return event.type === 'checkout.session.completed'
    && event.data.object.amount_total > 0
}
```

```typescript
// One adapter owns the external vocabulary.
type Money = { amountInCents: number; currency: string }

type PaymentReceived = {
  provider: 'stripe'
  paymentId: PaymentId
  amount: Money
  receivedAt: Date
}

function toPaymentReceived(event: Stripe.Event): PaymentReceived | null {
  if (event.type !== 'checkout.session.completed') return null

  // Stripe's event type does not narrow data.object, so assert it once here.
  const session = event.data.object as Stripe.Checkout.Session
  const paymentIntentId =
    typeof session.payment_intent === 'string'
      ? session.payment_intent
      : session.payment_intent?.id

  if (!paymentIntentId || session.amount_total === null) return null

  return {
    provider: 'stripe',
    paymentId: paymentId(paymentIntentId),
    amount: { amountInCents: session.amount_total, currency: session.currency ?? 'usd' },
    receivedAt: new Date(event.created * 1000),
  }
}
```

Rules:

- One module per external system owns its types, field names, retries, and error mapping.
- The domain exposes ports (interfaces) that describe what it needs, not which vendor provides it.
- Normalize units and formats at the boundary: cents to `Money`, provider statuses to domain states, timestamps to `Date`.
- Webhooks are input adapters. Verify the signature, translate, then call the domain.
- Do not wrap a library just to wrap it. Add a boundary when the dependency is likely to change, is vendor-specific, or would otherwise spread. A stable standard-library call needs no adapter.

**Check:** if the vendor is replaced, how many files change? The answer should be one adapter plus its tests.

**Tradeoff:** a boundary is extra code today for a cheaper change tomorrow. Skip it for a truly incidental dependency; add it for payments, auth, email, storage, and anything named in the business rules.

### Law 4 — Make Invalid States Harder to Represent

**Smell:** almost every field optional; `status: string`; flags that can contradict each other (`isPaid` and `isRefunded` both true); null checks repeated at every call site; "this should never happen" assertions.

**Move:** model the states the domain actually distinguishes, and make each state carry only the data it requires. Parse untrusted input into those types at the boundary.

```typescript
// Every combination is representable, including impossible ones.
type Order = {
  status: 'pending' | 'paid' | 'refunded'
  paidAt?: Date
  refundedAt?: Date
}
```

```typescript
// Each state carries exactly what it needs.
type OrderBase = { id: OrderId; customerId: CustomerId; amount: Money }

type PendingOrder = OrderBase & { status: 'pending'; createdAt: Date }
type PaidOrder = OrderBase & { status: 'paid'; paidAt: Date; paymentId: PaymentId }
type RefundedOrder = OrderBase & {
  status: 'refunded'
  paidAt: Date
  refundedAt: Date
  refundId: RefundId
}
type Order = PendingOrder | PaidOrder | RefundedOrder

function markRefunded(order: Order): RefundedOrder {
  switch (order.status) {
    case 'paid':
      return {
        ...order,
        status: 'refunded',
        refundedAt: new Date(),
        refundId: newRefundId(),
      }
    case 'pending':
      throw new DomainError('ORDER_NOT_PAID')
    case 'refunded':
      throw new DomainError('ORDER_ALREADY_REFUNDED')
  }
}
```

Rules:

- Optional means "may legitimately be absent in this state", not "I have not decided".
- Use a discriminated union, sealed class, or enum plus separate payload types — whichever the language supports.
- Validate at the boundary; trust the parsed type inside.
- Make constructors validating or private so a type cannot be built in an invalid state.
- Keep one source of truth. Do not mirror the same state in a boolean and a string.
- Do not model every theoretical invalid combination. Model the ones the business distinguishes and the code must branch on.

**Check:** can a caller construct the state you are about to defend against? If yes, fix the type instead of adding another check.

**Tradeoff:** precise types add ceremony at construction and serialization. Contain that cost in factories and mappers; do not leak it into every call site.

### Law 5 — Separate Decisions from Actions

**Smell:** a function that calculates a rule, writes to the database, sends an email, and publishes an event; tests that need a database and three mocks to verify a price; business rules duplicated between a UI preview and a server action.

**Move:** compute the decision as a pure function, then apply it. The rule becomes testable with plain values; the side effects become a thin, obvious shell.

```typescript
// The rule, the write, and the notification are one untestable unit.
async function approveRefund(orderId: string) {
  const order = await db.orders.find(orderId)
  if (!order) throw new Error('not found')
  if (order.status !== 'paid') throw new Error('not paid')
  if (Date.now() - order.paidAt.getTime() > 30 * DAY) throw new Error('too late')

  await db.refunds.insert({ orderId, amount: order.amount })
  await mailer.send(order.email, 'Refund approved')
}
```

```typescript
// The decision is pure and exhaustively testable.
type RefundDecision =
  | { kind: 'approve'; amount: Money }
  | { kind: 'reject'; reason: 'NOT_PAID' | 'WINDOW_EXPIRED' }

function decideRefund(order: Order, now: Date): RefundDecision {
  if (order.status !== 'paid') return { kind: 'reject', reason: 'NOT_PAID' }
  if (now.getTime() - order.paidAt.getTime() > 30 * DAY) {
    return { kind: 'reject', reason: 'WINDOW_EXPIRED' }
  }
  return { kind: 'approve', amount: order.amount }
}

// The action applies the decision and owns all I/O.
async function approveRefund(orderId: string, now = new Date()) {
  const order = await orders.find(orderId)
  if (!order) return { ok: false as const, reason: 'NOT_FOUND' as const }

  const decision = decideRefund(order, now)
  if (decision.kind === 'reject') {
    return { ok: false as const, reason: decision.reason }
  }

  await refunds.insert({ orderId, amount: decision.amount })
  await mailer.send(order.customerId, 'Refund approved')
  return { ok: true as const }
}
```

Rules:

- Pure functions take everything they need as arguments, including the clock and IDs. No hidden `Date.now()`, no global state.
- Return decisions as data; do not perform the action inside the rule.
- Keep the action layer boring: load, decide, apply, record, emit.
- Test the rule with a table of inputs and expected decisions. Test the action with one integration test, not twenty.
- Do not force purity onto code that has no rules. A simple CRUD write needs no decision layer.

**Check:** can you test the business rule without a database, network, clock, or mock? If not, the decision is still entangled.

**Tradeoff:** separate files and types for a rule with two branches can feel heavy. Introduce the split when the rule is important, changes often, or is duplicated; keep trivial mapping inline.

### Law 6 — Make Errors Useful

**Smell:** `throw new Error('Something went wrong')`; error strings parsed with `includes('not found')`; the same failure logged five times; a stack trace with no request or entity context; secrets, tokens, or full payloads in logs.

**Move:** every expected failure gets a stable code, a human message, safe context, and a preserved cause. Callers branch on the code, humans read the message, operators search the context.

```typescript
// The caller can only string-match, and the log leaks the payload.
throw new Error(`Failed to charge ${JSON.stringify(card)}`)
```

```typescript
// Stable code, safe context, preserved cause, no sensitive data.
class AppError extends Error {
  constructor(
    readonly code: ErrorCode,
    message: string,
    readonly context: Record<string, string | number> = {},
    options?: { cause?: unknown },
  ) {
    super(message, options)
    this.name = 'AppError'
  }
}

throw new AppError(
  'PAYMENT_DECLINED',
  'The payment was declined by the provider.',
  { orderId: order.id, provider: 'stripe' },
  { cause: providerError },
)
```

Rules:

- Codes are stable, uppercase, and documented: `PAYMENT_DECLINED`, `ORDER_NOT_FOUND`, `RATE_LIMITED`.
- Messages are for humans and contain no secrets, tokens, full card numbers, or personal data.
- Context is structured and searchable: IDs, counts, provider names, durations. Never credentials.
- Preserve the cause. Do not replace a useful low-level error with a vague high-level one.
- Distinguish expected failures (validation, conflict, authorization) from bugs (broken invariants, null dereferences). Expected failures are values or typed errors; bugs should crash loudly in development and be reported in production.
- Log once, at the boundary that can act on the error. Add context as it propagates; do not log-and-rethrow at every layer.
- Map domain errors to transport errors in one place: HTTP status, problem+json, gRPC code. Do not leak stack traces to clients.
- Expose a retryable flag or equivalent when callers must decide whether to retry.

**Check:** can a caller handle this failure without reading the message text? Can an operator find the failing request from the log without exposing user secrets?

**Tradeoff:** too many codes become an unmaintainable taxonomy. Group by what the caller must do differently, not by every internal cause.

### Law 7 — Keep Changes Focused

**Smell:** a pull request that renames files, upgrades a dependency, changes behavior, and reformats a module; a "small fix" with 60 files; a refactor mixed into a feature so neither can be reviewed or reverted independently.

**Move:** one change, one reason. Separate mechanical refactors from behavior changes. Separate dependency upgrades from feature work. Stack or split when the work is genuinely large.

Rules:

- The diff should have one answer to "why does this exist?"
- Refactor first, then change behavior, in separate commits or PRs. A reviewer should never guess which line changed behavior.
- Keep mechanical changes (rename, move, format) isolated; they are easy to verify when nothing else moves.
- If a change cannot be described without "and", split it.
- Size by cognitive load, not line count. A 400-line rename is easier to review than a 40-line change that alters three behaviors.
- Do not leave the codebase worse in the area you touched, but do not fix unrelated problems in the same change. Note them and file a follow-up.
- Every focused change should be independently deployable or revertible where the system allows it.

**Check:** can a reviewer state the change in one sentence and verify it without reading unrelated files? Can it be reverted without undoing something else?

**Tradeoff:** splitting has overhead. For a genuinely atomic change, one PR is correct. The goal is reviewability, not a line limit.

## The Next-Change Test

Pick the most likely next change to this code — new state, new provider, new rule, new caller. Then answer:

1. Which files must I open to make it?
2. Which tests must I change?
3. Which external system must I mock?
4. Which unrelated behavior could I accidentally break?

If any answer is "I don't know" or "most of them", the design is not ready. The laws are the tools for shrinking those answers.

## Order of Application

Laws interact. Apply them in this order when designing or untangling a flow:

1. **Scope (7)** — define the one change; keep cleanup separate.
2. **Boundary (3)** — decide what external data becomes once it enters.
3. **State (4)** — model the states the domain distinguishes.
4. **Decision (5)** — extract the rule from the I/O that applies it.
5. **Main path (1)** — flatten the resulting orchestration with guard clauses.
6. **Names (2)** — name the concepts that survived the previous steps.
7. **Errors (6)** — make every exit from the flow useful.

The order is deliberate. Naming before modeling produces names for concepts that should not exist. Error handling before boundary translation produces codes for vendor failures the domain should never see.

When laws conflict, prefer **correctness and safety > locality of change > readability > brevity**. A boundary type that adds a file but prevents a vendor change from touching thirty files wins. A guard clause that adds a return but removes a nesting level wins. A shorter name that loses meaning does not.

## Review Audit

Use this for self-review or diff review. Ask for evidence, not impressions.

| Law | Question | Evidence to look for | Typical fix |
|---|---|---|---|
| 1 Main path | Is the primary operation obvious at a glance? | Nesting depth, many exits, long functions | Guard clauses; extract the error path |
| 2 Naming | Do names express domain meaning? | Generic names, misleading booleans, comments explaining names | Rename; add a domain term |
| 3 Boundary | Is external vocabulary contained? | Vendor types in domain signatures, payload fields in rules | Adapter plus internal type at the edge |
| 4 States | Can impossible combinations be constructed? | Optional fields, string statuses, contradictory flags | Discriminated union; parse at boundary |
| 5 Decisions | Can rules be tested without I/O? | DB or mocks in a rule test, clock calls inside logic | Pure decision function; imperative shell |
| 6 Errors | Can callers act on failures? | String matching, vague messages, secret-bearing logs | Stable codes, safe context, cause |
| 7 Focus | Does the diff have one reason? | Mixed refactor and feature, unrelated files | Split or stack the change |

Stop at the first law that, if fixed, would make the others unnecessary. Do not produce a seven-point report when one structural change solves the problem.

## Language Notes

### TypeScript

- Parse at the boundary with a schema (Zod, Valibot, ArkType) into a domain type; do not cast `unknown` with `as`.
- Use discriminated unions for state and decisions; add an exhaustiveness check with `never` in the default branch.
- Prefer `readonly` fields and `ReadonlyArray` for values that must not be mutated downstream.
- Use branded types for identifiers (`type OrderId = string & { readonly __brand: 'OrderId' }`) when mixing IDs is a real risk.
- Avoid `any` at boundaries; `unknown` plus parsing is the honest type.
- Keep SDK types out of exported domain signatures.

### Python

- Use `dataclass(frozen=True)` or `NamedTuple` for value types; `Enum` or `Literal` for states.
- Use `Protocol` to define ports at the boundary, so the domain depends on a shape rather than a vendor class.
- Parse external payloads with Pydantic or explicit constructors at the edge; do not pass raw `dict` through the domain.
- Raise domain exceptions with a `code` attribute, or return a `Result` type where callers routinely branch. Do not mix both conventions in one module.
- Avoid `Optional` by default. Use it only when absence is a real state, and narrow it before use.
- Run mypy or pyright in strict mode for domain modules. Dynamic typing at the boundary is not an excuse for untyped internals.

### Go

- Accept interfaces, return structs. Define the interface where it is consumed, not where it is implemented.
- Decode into a dedicated external struct, validate it, then map to a domain struct. Do not decode a vendor payload straight into a domain type.
- Go has no sum types. Represent states with separate types, a sealed interface (unexported method), or a struct with a validating constructor; do not rely on a string plus booleans.
- Wrap errors with context: `fmt.Errorf("charge order %s: %w", orderID, err)`. Use `errors.Is` and `errors.As` for branching, not string matching.
- Define sentinel or typed errors for expected failures; keep them free of secrets.
- Treat the zero value as a state. If zero is invalid, require a constructor and validate it.
- Avoid `map[string]any` in domain code; it deletes the type system exactly where meaning matters.

## Rationalizations to Reject

| Excuse | Reality |
|---|---|
| "We'll clean it up later." | Later competes with the next feature. At minimum, contain the damage now with a boundary or a named type. |
| "It's only used in one place." | Boundaries are about the cost of change, not reuse. One place today can become thirty after the next feature. |
| "Optional fields are more flexible." | Optional fields push the same check into every caller. Flexibility for the producer is ambiguity for the consumer. |
| "The ticket is urgent." | Urgency justifies a smaller slice, not a mixed one. The fastest safe change is still focused. |
| "More files means more complexity." | File count is not cognitive load; hidden coupling is. A boundary file often reduces total complexity. |
| "Types are runtime overhead." | Parse once at the edge, then trust the type. The overhead is one validation, not one per call site. |
| "Errors are for logs." | Errors are an API for callers. If they must parse a message, the API is missing. |
| "Naming is bikeshedding." | Names are the interface to every future reader. A wrong name is a permanent tax. |
| "This abstraction might be useful later." | Speculative abstraction is not the same as a boundary. Build for the change you can name. |
| "The whole module needs a rewrite." | A rewrite is a scope decision, not a license to mix it with behavior changes. Separate them or do not start. |

## Verification Checklist

Before calling the work done:

- [ ] I can state the change in one sentence, and the diff contains only that change.
- [ ] A new teammate can find the main operation without tracing nested branches.
- [ ] Names describe domain meaning; no `data`, `result`, or `item` survives where a real concept exists.
- [ ] External payloads and SDK types stop at one adapter; the domain uses its own vocabulary.
- [ ] Each domain state carries the data it requires; impossible combinations are hard to construct.
- [ ] Business rules are testable without a database, network, clock, or mock.
- [ ] Every expected failure has a stable code, a safe message, structured context, and a preserved cause.
- [ ] No secret, token, password, or unnecessary personal data appears in an error or log.
- [ ] Tests cover the rule paths and the boundary translation, not only the happy path.
- [ ] The change can be reverted without undoing an unrelated improvement.

## Related Skills

- `code-simplification` — behavior-preserving cleanup once the design is sound.
- `code-reviewer` — structured diff review and reporting.
- `incremental-implementation` — slicing delivery into small, verifiable increments.
- `architecture-designer` — service boundaries, data ownership, and system-level decisions.
- `backend-craft` — production backend validation, error envelopes, and operational rigor.
- `security` — threat modeling, secret handling, and secure defaults.
- `testing` and `test-master` — test strategy and coverage for the rules and boundaries you extract.
