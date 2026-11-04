### Phase 17: Architecture Output and Deliverables

51. **Produce database architecture deliverables.** At the conclusion of every database design engagement, produce:
    - **Data architecture summary**: A concise document stating the data domain, access patterns, technology choices, and key design decisions.
    - **Access pattern catalog**: The complete numbered list from step 2, with the database technology, table/collection, and index that serves each pattern.
    - **Entity-relationship diagram**: Visual representation of all entities and their relationships.
    - **Physical schema definition**: Complete DDL (CREATE TABLE, CREATE INDEX, constraints) or equivalent for the chosen database, ready for implementation.
    - **Technology selection ADR**: For each database technology chosen, a decision record with context, decision, alternatives considered, and consequences.
    - **Capacity estimate**: Storage, compute, and connection requirements at current scale and projected growth.
    - **Migration plan**: If migrating from an existing system, the step-by-step migration procedure with rollback plan.
    - **Operational runbook outline**: Key monitoring metrics, alerting thresholds, backup/restore procedures, and common troubleshooting steps.
    - **Open questions**: Areas requiring further investigation, stakeholder input, or production data analysis before finalizing the design.