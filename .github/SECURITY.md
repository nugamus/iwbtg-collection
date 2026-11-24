# Security policy

## Reporting a vulnerability

Do not open a public issue for a security problem, and do not include a working exploit in a public PR or discussion.

Report it privately through GitHub's private vulnerability reporting: open the repository's **Security** tab and choose **Report a vulnerability**. If that form is not available, open a minimal public issue asking the maintainer for a private channel, and put no details in it.

A useful report includes:

- the affected component (client, server, package parser, script sandbox, CI);
- the commit or version you tested (the project has no releases yet; use a commit on `main`);
- steps to reproduce and the impact;
- logs or a proof of concept, with any real secret or personal data redacted.

## Scope

- The client on native and web (WASM).
- The server and replay verification.
- Game-package loading and validation: packages and their scripts are untrusted input.
- The `.iwbz` replay format.

## Secrets

The repository history has been scanned with gitleaks and contains no known secrets. If you find one, report it privately as above so it can be rotated before it is discussed.
