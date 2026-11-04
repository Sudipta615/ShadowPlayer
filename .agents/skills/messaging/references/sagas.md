# Phase 10: Distributed Workflows and Sagas

23. **Design saga patterns for distributed transactions.** When a business operation spans multiple services (e.g., place order → reserve inventory → process payment → confirm order), and each step involves a different service with its own database, traditional transactions cannot span all services. Use sagas:

    **Choreography (event-driven sagas)**:
    - Each service reacts to events and publishes its own events. There is no central coordinator.
    - Flow: Order service publishes `OrderPlaced` → Inventory service reacts: reserves stock, publishes `StockReserved` → Payment service reacts: processes payment, publishes `PaymentCompleted` → Order service reacts: confirms order.
    - **Compensating actions**: If payment fails, the payment service publishes `PaymentFailed` → Inventory service reacts: releases stock reservation. Each service defines its own compensating action.
    - **Advantages**: Fully decoupled. No single point of failure. Each service is autonomous.
    - **Disadvantages**: Hard to understand the overall workflow (logic is scattered across services). Hard to add new steps or change the order. Hard to handle complex failure scenarios (what if stock reservation times out — who retries? who compensates?). No single place to see the saga's current state.
    - **When to use**: Simple workflows with 2-3 steps and straightforward compensation. When services are truly independent and the workflow is unlikely to change.

    **Orchestration (centralized saga coordinator)**:
    - A dedicated orchestrator (workflow engine) coordinates the saga. The orchestrator sends commands to each service and handles responses, retries, timeouts, and compensating actions.
    - Flow: Saga orchestrator sends `ReserveStock` command to inventory service → receives `StockReserved` response → sends `ProcessPayment` command to payment service → receives `PaymentCompleted` response → sends `ConfirmOrder` command to order service.
    - If any step fails: orchestrator executes compensating actions in reverse order.
    - **Advantages**: The workflow is defined in one place (readable, testable, modifiable). Complex workflows with branching, parallel steps, and timeouts are manageable. Easy to add new steps. Easy to monitor saga state.
    - **Disadvantages**: The orchestrator is a potential single point of failure (must be highly available). Coupling between the orchestrator and participant services.
    - **When to use**: Workflows with more than 3 steps. Workflows with complex failure handling, timeouts, or conditional branching. When visibility into workflow state is important.
    - **Implementation**: Use a workflow engine — Temporal (recommended for new systems), AWS Step Functions, Cadence, Netflix Conductor, or Camunda. These provide: durable execution (survives crashes), automatic retries, timeouts, state persistence, and observability. **Do not build a custom saga orchestrator** — the edge cases (partial failures, timeouts, concurrent compensations, idempotency) are extremely complex and have been solved by purpose-built tools.

24. **Design compensating actions.** Every saga step that modifies state must have a defined compensating action:

    | Saga Step | Action | Compensating Action |
    |---|---|---|
    | Reserve inventory | Decrement available stock | Increment available stock (release reservation) |
    | Process payment | Charge customer's payment method | Refund the charge |
    | Create shipment | Schedule pickup | Cancel shipment |
    | Send confirmation email | Send email | Send cancellation/correction email (best effort) |

    **Compensating action rules**:
    - Compensating actions must be idempotent (they may be executed more than once if the compensation itself fails and retries).
    - Some actions cannot be perfectly compensated (e.g., a sent email cannot be unsent). Design these as "best effort" compensations (send a correction email) and document the limitation.
    - Compensating actions should execute in reverse order of the original actions.
    - Set timeouts on each saga step. If a step does not complete within the timeout, trigger compensation rather than waiting indefinitely.