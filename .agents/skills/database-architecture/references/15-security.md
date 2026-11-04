### Phase 15: Database Security

46. **Design database access control.** Define the security model:
    - **Principle of least privilege**: Each application connects with a database user that has only the permissions it needs. The application user should NOT be a superuser.
    - **Define database roles**:
      - `app_readwrite`: SELECT, INSERT, UPDATE, DELETE on application tables. Used by the application service.
      - `app_readonly`: SELECT only. Used by read replicas, reporting tools, and analytics queries.
      - `migration_admin`: DDL permissions (CREATE, ALTER, DROP) on application schemas. Used only by migration tools, not by the running application.
      - `monitoring_readonly`: SELECT on system catalogs and statistics views. Used by monitoring agents.
    - **Row-Level Security (RLS)**: For multi-tenant systems, enable RLS and define policies that restrict access to rows matching the current tenant. This provides defense-in-depth — even if application code omits a tenant filter, the database rejects cross-tenant access.
    - **Network access**: Database must not be publicly accessible. Restrict access to application VPC/subnet. Use security groups / firewall rules to allow only known application servers and authorized admin IPs.
    - **Audit logging**: Enable database audit logging (pgAudit for PostgreSQL) for: DDL changes, privilege changes, and access to sensitive tables. Store audit logs separately from the database with tamper-resistant retention.

47. **Design encryption.** Define:
    - **Encryption at rest**: Enable storage-level encryption (AWS RDS encryption, GCP Cloud SQL encryption). This is transparent to the application and protects against physical media theft. Use customer-managed keys (KMS) for compliance requirements.
    - **Encryption in transit**: Enforce TLS for all database connections. Configure `sslmode=verify-full` on application connections (verify the database server's certificate) to prevent MITM attacks.
    - **Field-level encryption**: For highly sensitive data (SSN, credit card numbers, health records) that must be protected even from database administrators: encrypt specific fields in the application before storage. Define the key management strategy (KMS, Vault), key rotation procedure, and the impact on queryability (encrypted fields cannot be indexed or queried at the database level — design access patterns accordingly).
    - **Backup encryption**: All backups must be encrypted at rest with keys stored separately from the backup storage.