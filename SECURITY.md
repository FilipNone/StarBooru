# Security policy

## Reporting a vulnerability

Please don't open a public issue. Report it privately through [GitHub's vulnerability reporting](https://github.com/FilipNone/StarBooru/security/advisories/new) with the steps to reproduce it, what an attacker could do, and the version or commit you tested.

StarBooru is a personal project, so there is no fixed response time. Reports are read and fixes go out as soon as they can.

## Supported versions

Only the latest release and the latest code on `main` get fixes.

## What counts

Collections are private, so the most serious problems are:

- reading or changing another account's media, or reaching media without a valid signed URL;
- signing in as someone else, or getting more rights than an account or API key should have;
- making the server fetch addresses it should refuse (URL import, other boorus);
- a malformed upload or request that crashes the server;
- tokens, passwords or file contents ending up in logs.

Known limits are listed under *Known gaps and risks* in [docs/roadmap.md](docs/roadmap.md).
