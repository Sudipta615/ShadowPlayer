# Data Protection and Cryptography

This reference covers **Phase 5: Data Protection and Cryptography**. See the main SKILL.md for the phase summary and link.

---

12. **Design security headers.** Configure HTTP security headers on all responses. These are low-cost, high-impact defenses:

    ```
    Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
    Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self' https://api.example.com; frame-ancestors 'none'; base-uri 'self'; form-action 'self'
    X-Content-Type-Options: nosniff
    X-Frame-Options: DENY
    Referrer-Policy: strict-origin-when-cross-origin
    Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=()
    Cache-Control: no-store (for sensitive responses)
    ```

    - **HSTS** (`Strict-Transport-Security`): Forces browsers to use HTTPS for all future requests to the domain. Set `max-age` to at least 1 year (31536000). Include `includeSubDomains` to cover all subdomains. Add `preload` and submit to the HSTS preload list for maximum protection. **Warning**: Enabling HSTS is effectively irreversible for the max-age duration — ensure HTTPS is fully functional before enabling.
    - **CSP** (`Content-Security-Policy`): The most powerful browser security header. Restricts which sources can load scripts, styles, images, etc. Design the CSP iteratively: start with `Content-Security-Policy-Report-Only` to collect violations without blocking, fix violations, then enforce. Key directives:
      - `default-src 'self'`: Block all external sources by default.
      - `script-src 'self'`: Only allow scripts from your own origin. Avoid `'unsafe-inline'` (blocks most XSS). If inline scripts are needed, use nonces (`'nonce-{random}'`) or hashes.
      - `frame-ancestors 'none'`: Prevents clickjacking (replaces X-Frame-Options). Set to `'self'` if the app is legitimately iframed within the same origin.
      - `base-uri 'self'`: Prevents base tag injection.
      - `form-action 'self'`: Prevents form submissions to external origins.
      - `report-uri` or `report-to`: Send CSP violation reports to a monitoring endpoint for analysis.
    - **X-Content-Type-Options**: `nosniff` prevents browsers from MIME-sniffing responses to a different content type than declared. Prevents execution of uploaded files as scripts.
    - **Referrer-Policy**: `strict-origin-when-cross-origin` prevents leaking the full URL path to third-party sites while maintaining referrer information for same-origin requests.
    - **Permissions-Policy**: Disables browser features the application does not use (camera, microphone, geolocation). Reduces the attack surface if XSS is exploited.

13. **Design encryption at rest.** Protect stored data against physical theft, unauthorized access to storage media, and database compromise:

    **Storage-level encryption** (transparent disk encryption):
    - Enable encryption at rest on all storage volumes, databases, and object storage buckets. All major cloud providers offer this: AWS EBS/RDS/S3 encryption, GCP CMEK, Azure Storage encryption.
    - Use customer-managed keys (CMK) via KMS when compliance requires key control, audit, and the ability to revoke access by destroying the key. Use provider-managed keys when operational simplicity is preferred and compliance permits.
    - Storage-level encryption protects against physical media theft and unauthorized access to the underlying storage infrastructure. It does not protect against application-level data breaches (an attacker who compromises the application sees decrypted data because the application has access to the decryption key).

    **Application-level encryption** (field-level or envelope encryption):
    - Encrypt specific highly sensitive fields (SSN, credit card numbers, health records, financial data) in the application layer before storing them. The database stores ciphertext.
    - **Envelope encryption pattern**: Generate a data encryption key (DEK) → encrypt the data with the DEK (AES-256-GCM) → encrypt the DEK with a key encryption key (KEK) stored in KMS → store the encrypted data and encrypted DEK together → to decrypt, call KMS to decrypt the DEK, then decrypt the data.
    - **Algorithm selection**: AES-256-GCM (authenticated encryption — provides both confidentiality and integrity). Never use AES-ECB (reveals patterns in the ciphertext). Never use AES-CBC without a separate HMAC for integrity (vulnerable to padding oracle attacks). AES-GCM is the default recommendation.
    - **Tradeoff**: Application-level encryption prevents database administrators, backup operators, and anyone with storage access from reading the data. But encrypted fields cannot be searched or indexed at the database level (unless using techniques like deterministic encryption for exact-match lookups — which leaks equality patterns — or searchable encryption schemes, which are complex and have performance costs). Design access patterns accordingly.
    - **Key rotation**: Design the system to support key rotation without re-encrypting all data immediately. Envelope encryption enables this — rotate the KEK in KMS, re-encrypt only the DEKs (small, fast), and the data remains encrypted with the same DEKs. For field-level encryption, tag each ciphertext with the key version used so the correct key is used for decryption. Re-encrypt data in the background over time.

14. **Design encryption in transit.** Protect data against network interception, modification, and eavesdropping:

    **TLS configuration**:
    - **TLS 1.2 minimum**, TLS 1.3 preferred. Disable TLS 1.0 and TLS 1.1 — they have known vulnerabilities. Disable SSL entirely.
    - **Cipher suite configuration** (TLS 1.2): Use only AEAD cipher suites with forward secrecy:
      - Recommended: `TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384`, `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`, `TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384`.
      - Disable: RC4, DES, 3DES, CBC-mode ciphers (vulnerable to BEAST, POODLE), non-PFS ciphers (RSA key exchange without ECDHE).
    - TLS 1.3 simplifies this — only secure cipher suites are available. If all clients support TLS 1.3, use it exclusively.
    - **Certificate management**: Use certificates from a trusted CA (Let's Encrypt for public endpoints, internal CA for internal services). Automate certificate renewal (cert-manager in Kubernetes, AWS ACM, Let's Encrypt certbot). Monitor certificate expiry — expired certificates cause outages. Alert at least 30 days before expiry.
    - **HSTS**: As described in step 12.
    - **Internal traffic**: Encrypt internal service-to-service traffic. In zero-trust architectures, use mutual TLS (mTLS) via a service mesh (Istio, Linkerd) or manually configured certificates. Even within a "trusted" VPC, network segmentation can be breached — encryption in transit protects against lateral movement.

    **Certificate pinning** (for mobile apps, use with caution):
    - Pin the CA certificate (not the leaf certificate) to prevent MITM attacks even if a rogue CA issues a fraudulent certificate.
    - Include backup pins for certificate rotation.
    - Warning: Certificate pinning makes certificate rotation difficult. A mistake can lock users out of the app. Use only if the threat model justifies it (e.g., high-security financial apps). Prefer Certificate Transparency monitoring as a less disruptive alternative.

15. **Design data masking and tokenization.**

    **Data masking** (for non-production environments and logs):
    - Define masking rules for each sensitive data type:
      - Email: `j***@example.com`
      - Phone: `***-***-1234`
      - Credit card: `****-****-****-5678`
      - SSN: `***-**-6789`
      - Name: `J*** D***` or full redaction.
    - Apply masking automatically in: log output (see the Application Security reference), non-production database copies, customer support interfaces (unless the support agent has explicit authorization to see unmasked data), analytics/reporting pipelines.
    - **Production database copies for non-production**: Never copy production data to development/staging without anonymization or masking. Use a data anonymization pipeline that replaces real PII with synthetic data while preserving data relationships and distributions for testing.

    **Tokenization** (for PCI-DSS and sensitive data storage):
    - Replace sensitive data (credit card numbers, bank account numbers) with a non-reversible token that maps to the original value in a secure token vault.
    - The token vault is a separate, highly secured system with strict access controls.
    - Tokenization reduces PCI-DSS scope — systems that only handle tokens (not card numbers) are out of scope for many PCI requirements.
    - Use a managed tokenization service (Stripe, payment processor tokenization) whenever possible rather than building custom tokenization.

16. **Design key management.** Cryptographic keys are the foundation of all encryption. Mismanaged keys invalidate all other encryption efforts:

    - **Never store encryption keys alongside the encrypted data.** An encrypted database with the decryption key in the application's environment variables is security theater — any breach that reaches the data also reaches the key.
    - **Use a dedicated Key Management Service (KMS)**: AWS KMS, GCP Cloud KMS, Azure Key Vault, or self-hosted HashiCorp Vault. KMS provides: hardware-backed key storage (HSM), access policies, automatic key rotation, audit logging of key usage.
    - **Key hierarchy**: Use envelope encryption (step 13). KMS manages the root/master keys. Application-generated DEKs encrypt the data. The DEKs are encrypted by the KMS key. This limits KMS API calls (DEK is cached in memory for the encryption session) while maintaining security.
    - **Key rotation schedule**: Rotate KMS keys annually at minimum (or per compliance requirements). With envelope encryption, KEK rotation does not require re-encrypting all data — only re-wrapping DEKs.
    - **Key access policies**: Only services that need to encrypt/decrypt specific data should have access to the corresponding KMS keys. Define IAM policies per service per key.
    - **Key destruction**: When data must be permanently destroyed (right to be forgotten), destroying the encryption key renders the data irrecoverable without needing to overwrite every copy. This is "crypto-shredding." Design the key hierarchy to support per-tenant or per-user keys if crypto-shredding is a requirement.