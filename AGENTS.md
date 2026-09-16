# AGENT DIRECTIVES & OPERATIONAL CONTRACT

## 1. CRITICAL CONSTRAINTS (ZERO TOLERANCE)
Violation of these rules will corrupt the environment and block user interaction.

- **NEVER EXECUTE THE PROJECT:** Do not run binaries, test runners, or entrypoints under any circumstances.
- **NEVER RUN `cargo build`:** Compilations lock the interface. Code validation must strictly and exclusively use `cargo check` or `cargo clippy`.
- **NO DIRECT DATABASE ACCESS:** Never attempt to connect to, query, migrate, or inspect databases (Oracle, SQLite, etc.). If external data or schemas are required, **halt and request manual retrieval by the user**.
- **EXECUTION BOUNDARY:** When in doubt regarding side effects, emit the proposed command as text for manual verification rather than executing it.

---

## 2. TOOLING PIPELINE (STRICT REPLACEMENTS)
Avoid legacy GNU/POSIX utilities. Always adhere to the modern CLI toolchain specified below, fallback to powershell if the tools are not available:

| Task | Mandatory Tool | Prohibited Legacy |
| :--- | :--- | :--- |
| **File Search** | `fd` | `find`, `dir` |
| **Pattern Search** | `rg` (ripgrep) | `grep` |
| **Text Replacement** | `sd` | `sed`, `awk` |
| **Code Diagnostics** | `cargo check`, `cargo clippy` | `cargo build`, `cargo run` |

---