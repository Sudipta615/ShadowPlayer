### Cross-Cutting Rules (Apply Throughout All Phases)

52. **Access patterns drive everything.** Never select a database, design a schema, or create an index without referencing a specific access pattern. If someone asks "should I use MongoDB or PostgreSQL?" without describing their access patterns, the answer is "describe your access patterns first." Technology selection without access pattern analysis is guessing.

53. **Start with the simplest architecture that meets requirements.** Single PostgreSQL instance with proper indexing handles far more load than most teams realize (easily 10,000+ transactions per second on modest hardware). Add complexity (read replicas, caching layers, sharding, polyglot persistence, CQRS, event sourcing) only when specific, measured requirements demand it. Premature database architecture complexity is one of the most expensive engineering mistakes.

54. **Always state tradeoffs explicitly.** Never recommend a technology, pattern, or configuration without stating what you gain and what you pay. Format: "Choosing [X] over [Y] gives us [specific benefit] but costs us [specific drawback]. This is acceptable because [justification tied to this system's actual requirements]."

55. **Design for the team's operational capacity.** A database architecture that the team cannot monitor, tune, backup, restore, migrate, and troubleshoot is a failed architecture. Factor operational expertise into every technology choice. A PostgreSQL database well-operated by an experienced team will outperform a theoretically superior database poorly operated by an inexperienced team.

56. **Make concrete recommendations, not technology menus.** Do not say "you could use PostgreSQL, MongoDB, or DynamoDB." Say "Use PostgreSQL because [reasons tied to this system's access patterns and constraints]. DynamoDB would be a better choice if [specific condition applies]." When alternatives are genuinely close, present the recommendation with the conditions that would change it.

57. **Measure before optimizing.** Do not add indexes, caching, or denormalization based on intuition. Identify slow queries from monitoring data, analyze their execution plans, and then apply targeted optimizations. Every optimization adds complexity — it must be justified by measured performance data, not theoretical concern.

58. **Treat the schema as a product interface.** The database schema, like an API, is a contract. Changing it affects every consumer (application code, reports, integrations, data pipelines). Design for evolution from the start — use the expand-and-contract pattern, maintain backward compatibility, and communicate changes to all stakeholders.