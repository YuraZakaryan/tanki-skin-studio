# Security policy

Tanki Skin Studio writes to files on your computer (a game cache and its own data folder), so a
security problem in it matters even though it never goes online.

## Supported versions

Only the latest release and the latest code on `main` get security fixes.

## Reporting a vulnerability

**Please do not open a public issue.** Report it privately through GitHub: open the repository's
**Security** tab and choose **Report a vulnerability**. Include what you found, the steps or a file
that shows it, and what an attacker could do with it. Never attach game files.

What happens next:

- you get an answer within **7 days**;
- a confirmed problem gets a fix or a plan within **30 days**, and you are kept up to date;
- the fix is published together with a security advisory that credits you, unless you prefer not
  to be named.

## In scope

- reading or writing outside the chosen cache folder or the app's data folder;
- a pack, archive or cache file that crashes the app, hangs it or exhausts memory or disk;
- an apply or restore that leaves the cache damaged or cannot be undone;
- anything that makes the app reach the network.
