# Application Security

This reference covers **Phase 3: Application Security**. See the main SKILL.md for the phase summary and link.

---

5. **Design input validation and injection prevention.** Injection attacks remain the most common and dangerous application vulnerability class. Design defense at every input point:

   **SQL Injection prevention**:
   - **Parameterized queries / prepared statements** (mandatory): Never concatenate user input into SQL strings. Every database interaction must use parameterized queries:
     ```
     ✗ WRONG: query = f"SELECT * FROM users WHERE id = {user_input}"
     ✓ RIGHT: query = "SELECT * FROM users WHERE id = $1", [user_input]
     ```
   - ORMs with parameterized queries are safe by default, but raw SQL escape hatches in ORMs must still use parameters.
   - **Stored procedures** do not prevent SQL injection unless they also use parameterized queries internally.
   - **Allow-list validation for dynamic identifiers**: If column names, table names, or sort orders must be dynamic (user-selected), validate against an explicit allow-list of permitted values. Never interpolate user input into SQL identifiers.
   - **Database user least privilege**: The application's database user should have only SELECT, INSERT, UPDATE, DELETE on the specific tables it needs. Never GRANT ALL PRIVILEGES or use the superuser account. This limits the damage of a successful injection.

   **NoSQL Injection prevention**:
   - MongoDB and similar: Never pass raw user input into query operators. Validate that query parameters are the expected types (string, number, not objects). Use MongoDB driver's built-in query builder, not string concatenation. Block operator injection: if the user can pass `{"$gt": ""}` as a username, they can bypass authentication.

   **Command Injection prevention**:
   - **Never pass user input to shell commands** (`exec`, `system`, `os.popen`, `subprocess.shell=True`). If shell execution is absolutely necessary, use parameterized command execution (`subprocess.run([cmd, arg1, arg2], shell=False)` in Python) and validate every argument against an allow-list.
   - Prefer language-native libraries over shell commands. Example: use a PDF library instead of shelling out to `wkhtmltopdf` with user-controlled parameters.

   **LDAP Injection prevention**:
   - Use parameterized LDAP queries. Escape special characters in user input (`*`, `(`, `)`, `\`, NUL).

   **XSS (Cross-Site Scripting) prevention**:
   - **Output encoding** (primary defense): Encode all user-supplied data when rendering in HTML, JavaScript, CSS, or URL contexts. Use context-appropriate encoding (HTML entity encoding for HTML body, JavaScript encoding for inline scripts, URL encoding for URL parameters). Framework auto-escaping (React JSX, Django templates, Go html/template) handles most cases — never disable auto-escaping.
   - **Content Security Policy (CSP)** (defense in depth): See the Data Protection and Cryptography reference.
   - **Sanitization for rich text**: If the application accepts HTML input (rich text editors), use a server-side HTML sanitizer with an allow-list of permitted tags and attributes (e.g., DOMPurify on the client, Bleach in Python, Sanitize in Ruby). Never use a deny-list approach — there are always bypasses.
   - **DOM-based XSS**: Avoid using `innerHTML`, `document.write`, or `eval()` with user-controlled data. Use `textContent` for inserting text. Use framework-provided rendering methods.

   **Server-Side Request Forgery (SSRF) prevention**:
   - If the application fetches URLs based on user input (webhooks, URL previews, file imports), validate the URL:
     - **Allow-list approach** (strongest): Only allow requests to known, pre-approved domains/IPs.
     - **Deny-list approach** (weaker but necessary): Block requests to internal IP ranges (127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.169.254 — AWS metadata), localhost, and internal hostnames. Block non-HTTP(S) schemes (file://, gopher://, ftp://).
     - Resolve the hostname and validate the resolved IP against the deny-list (prevents DNS rebinding where a hostname initially resolves to a public IP but later resolves to an internal IP).
     - Use a dedicated, isolated network egress point for outbound requests to user-supplied URLs (a proxy in a separate VPC/network segment with no access to internal services).
     - Set timeouts on outbound requests to prevent SSRF from being used for denial-of-service.

   **Path Traversal prevention**:
   - Never use user input directly in file paths. If the application serves or processes files based on user-supplied filenames:
     - Validate the filename against an allow-list of characters (alphanumeric, limited special characters).
     - Reject paths containing `..`, `/`, `\`, null bytes.
     - Resolve the final path and verify it is within the expected base directory (`realpath(path).startswith(base_dir)`).
     - Use a unique, system-generated filename (UUID) rather than the user-supplied filename for storage.

   **Mass Assignment prevention**:
   - Explicitly define which fields are writable for each API endpoint. Never blindly map request body fields to database columns/model attributes.
   - Use DTOs (Data Transfer Objects) or serializer allow-lists to control which fields are accepted from external input.
   - Example threat: User sends `{"role": "admin", "name": "attacker"}` on a profile update endpoint. If mass assignment is not prevented, the user escalates their privileges.

6. **Design output security.** Prevent sensitive data leakage in responses:
   - **Error messages**: Never expose internal details in error responses — stack traces, database error messages, file paths, framework versions, or SQL queries. Return generic error messages to the client ("An internal error occurred. Reference: REQ-abc123.") and log detailed errors server-side with the request ID.
   - **Response filtering**: Define explicit response schemas for each API endpoint. Never return `SELECT *` results directly — map database rows to response DTOs that exclude internal fields (internal IDs, soft-delete flags, internal timestamps, admin-only fields).
   - **HTTP headers**: Remove server identification headers (`Server`, `X-Powered-By`, `X-AspNet-Version`). These reveal technology stack information useful for targeted attacks.
   - **Debug endpoints**: Ensure all debug endpoints (profiling, configuration dumps, health checks with sensitive details) are disabled in production or protected behind strong authentication and network restrictions.
   - **API error differentiation**: Carefully consider whether error messages should differentiate between "resource not found" and "resource exists but you don't have access." In many cases, returning 404 for both prevents information disclosure about resource existence. Document this decision.

7. **Design file upload security.** File uploads are a high-risk attack surface:
   - **Validate file type server-side**: Check the file's magic bytes (file header), not just the file extension or `Content-Type` header (both are user-controlled and trivially spoofed). Use a library that performs magic byte detection.
   - **Allow-list permitted file types**: Only accept file types the application legitimately needs (e.g., JPEG, PNG, PDF). Reject everything else.
   - **Limit file size**: Enforce maximum file size at both the reverse proxy/load balancer level and the application level. Prevent denial-of-service through oversized uploads.
   - **Rename uploaded files**: Store files with a system-generated name (UUID). Never use the original filename for storage — it may contain path traversal sequences, special characters, or excessively long names.
   - **Store uploads outside the web root**: Never serve uploaded files from the same directory tree as application code. Use object storage (S3, GCS) with signed URLs for access, or a dedicated file-serving domain.
   - **Scan for malware**: For applications accepting user-uploaded files (especially those shared with other users), integrate antivirus/malware scanning (ClamAV, cloud-based scanning APIs).
   - **Serve uploads from a separate domain**: Serve user-uploaded content from a different domain (e.g., `uploads.example-cdn.com`, not `example.com`) to prevent stored XSS in uploaded HTML/SVG files from having access to the application's cookies and origin. Set `Content-Disposition: attachment` for file downloads to prevent inline rendering.
   - **Image processing**: If the application processes uploaded images (resizing, thumbnailing), use a well-maintained image processing library. Image processing libraries have historically been a source of critical vulnerabilities (ImageTragick, libpng exploits). Keep them updated. Run image processing in a sandboxed environment if possible.

8. **Design deserialization security.** Insecure deserialization is a critical vulnerability that can lead to remote code execution:
   - **Never deserialize untrusted data with native serialization formats** (Java ObjectInputStream, Python pickle, PHP unserialize, Ruby Marshal). These formats can instantiate arbitrary objects and execute code during deserialization.
   - **Use safe data formats**: JSON, Protocol Buffers, MessagePack, or other formats that only represent data structures (not executable objects).
   - **If native serialization is unavoidable**: Implement strict allow-lists of permitted classes for deserialization. Use framework-provided security controls (Java's ObjectInputFilter). Keep serialization libraries updated.
   - **Validate deserialized data**: After deserialization, validate all fields as if they were external input. Deserialized data is not inherently trustworthy.