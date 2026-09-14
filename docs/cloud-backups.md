# Cloud backups

Industrialis Launcher can upload completed Minecraft backup archives to a cloud
provider. Google Drive is the first supported provider. S3-compatible storage,
OneDrive, FTP, FTPS, and SFTP are **Coming soon** and cannot be connected yet.

This guide was last checked against Google's documentation on 2026-08-22.

## Google Drive setup

Each user currently supplies their own Google Cloud OAuth client ID. You need a
Google Cloud project, the Google Drive API, and a **Desktop app** OAuth client.
You do not need a service account, API key, OAuth client secret, or downloaded
credentials JSON file.

### 1. Create or select a Google Cloud project

Open the [Google Cloud console](https://console.cloud.google.com/) and create a
project, or select a project dedicated to Industrialis Launcher. Keep the same
project selected for every step below.

### 2. Enable the Google Drive API

Open the Google Cloud API Library, find **Google Drive API**, and select
**Enable**. Google documents the console flow in
[Enable the Drive API](https://developers.google.com/workspace/drive/api/guides/enable-sdk#enable_the_drive_api).

### 3. Configure the Google Auth Platform

Open **Google Auth Platform** in the selected project. If the project has not
been registered for OAuth yet, select **Get started**, then configure:

- **Branding:** use a recognizable app name such as `Industrialis Launcher`,
  choose a monitored user-support email, and provide the requested developer
  contact email.
- **Audience:** choose **Internal** only when the project belongs to a Google
  Workspace organization and every account that will connect belongs to that
  organization. Otherwise choose **External**.
- **Data Access:** add only
  `https://www.googleapis.com/auth/drive.file`.

Google's current console sections and required app information are described in
[Get started with the Google Auth Platform](https://support.google.com/cloud/answer/15544987),
[Manage App Audience](https://support.google.com/cloud/answer/15549945), and
[Manage App Data Access](https://support.google.com/cloud/answer/15549135).

#### External apps and test users

If the External app has a publishing status of **Testing**, add every Google
Account that will connect under **Audience > Test users**. Testing is limited to
the listed test users. More importantly for automatic backups, Google expires a
test user's authorization and refresh token after seven days when the app asks
for Drive access. Expect to reconnect every seven days while the app remains in
Testing.

For normal long-running backups, change the app to **In production** when you
are ready and complete any console prompts that apply to your project. The
launcher requests only `drive.file`, which Google classifies as a recommended,
non-sensitive, per-file scope. Google's
[audience documentation](https://support.google.com/cloud/answer/15549945)
explains Testing and In production behavior, while
[Choose Google Drive API scopes](https://developers.google.com/workspace/drive/api/guides/api-specific-auth)
documents the scope classification and verification implications.

### 4. Create a Desktop app OAuth client

1. Open **Google Auth Platform > Clients**.
2. Select **Create client**.
3. For **Application type**, select **Desktop app**.
4. Enter a descriptive name such as `Industrialis Launcher Desktop`.
5. Select **Create**.
6. Copy the generated **Client ID**. It normally ends in
   `.apps.googleusercontent.com`.

Do not create a Web application client and do not paste a client secret into
the launcher. Installed desktop applications are public OAuth clients and
cannot safely keep a client secret. See Google's official
[OAuth client creation steps](https://developers.google.com/workspace/guides/create-credentials#desktop-app)
and [OAuth client guidance](https://support.google.com/cloud/answer/15549257).

### 5. Connect Industrialis Launcher

1. Open Industrialis Launcher.
2. Open **Launcher Settings > Backups**.
3. Paste the Google OAuth Desktop app **Client ID** into the Google Drive card.
4. Select **Save ID**.
5. Select **Connect**.
6. In the browser window, sign in with an account allowed by the OAuth
   audience. If the app is in Testing, use an account listed as a test user.
7. Review the requested Drive access and approve it. The browser should report
   that Google Drive is connected and can then be closed.
8. For each Minecraft instance to protect, open **Instance Settings > General**
   and enable **Cloud backups**.

The launcher listens on a temporary `127.0.0.1` port to receive the OAuth
response, then closes that listener. This is Google's recommended loopback
redirect method for Windows, macOS, and Linux desktop apps. The authorization
also uses PKCE. See
[OAuth 2.0 for Desktop Apps](https://developers.google.com/identity/protocols/oauth2/native-app).

## Expected behavior

- The launcher watches each enabled instance's `.minecraft/backups` folder. It
  uploads regular backup files after they have remained unchanged for 60
  seconds. Temporary or partial files are ignored.
- On initial setup, the launcher backfills the newest distinct backups up to
  the configured retention limit. The default is 10 snapshots per provider.
- Google Drive contains a visible **Industrialis Backups** folder. Its managed
  layout is
  `instances/<instance-id>/snapshots/<sha256>/`, containing the archive under
  `artifacts/` and a `manifest.json` written after the archive finishes.
- The `drive.file` scope permits Industrialis to create and manage the Drive
  files it creates or that a user explicitly shares with it. It does not grant
  broad access to every file in the account. Google recommends this narrow
  scope for per-file access; see
  [Choose Google Drive API scopes](https://developers.google.com/workspace/drive/api/guides/api-specific-auth).
- Backup archives are not additionally encrypted by Industrialis. Do not put
  secrets in a Minecraft world archive if this protection model is unsuitable
  for you.
- The OAuth client ID is an app identifier, not a password. The account refresh
  token is encrypted using the operating system's secure credential storage.
  Google also recommends keeping refresh tokens in secure, long-term storage.
- Disconnecting Google Drive removes the launcher's locally stored connection
  token. It does not delete the **Industrialis Backups** folder or existing
  cloud snapshots.
- Retention deletes older cloud snapshots only. It never deletes local files
  from the instance's backups folder.
- Restoring downloads an archive back into `.minecraft/backups`; it never
  replaces a live Minecraft world automatically.

## Troubleshooting

### Connect is disabled

Paste the complete Desktop app client ID and select **Save ID** first. A client
secret or the contents of a downloaded JSON credentials file are not valid
values.

### `access_denied`, `access blocked`, or the account cannot authorize

Check **Google Auth Platform > Audience**. For an External app in Testing, add
the exact Google Account under **Test users**. For an Internal app, the account
must belong to the project's Google Workspace organization. See
[Manage App Audience](https://support.google.com/cloud/answer/15549945).

Google Workspace administrators can also restrict third-party OAuth apps. If a
managed work or school account is blocked, ask its administrator to review the
client ID under **Security > Access and data control > API controls**. Google's
[Workspace app-access documentation](https://support.google.com/a/answer/7281227)
describes those controls.

### The connection stops working after seven days

This is expected for an External OAuth app whose publishing status is Testing.
Add the account as a test user, reconnect, and move the app to In production
when appropriate for ongoing backups. Google documents the seven-day testing
refresh-token lifetime in
[Using OAuth 2.0 to Access Google APIs](https://developers.google.com/identity/protocols/oauth2#expiration).

### `redirect_uri_mismatch`

The OAuth client was likely created with the wrong application type. Create a
new **Desktop app** client and paste its client ID into the launcher. Do not add
a fixed redirect URI: the desktop flow uses a random local loopback port.
Google's [installed-app OAuth documentation](https://developers.google.com/identity/protocols/oauth2/native-app#redirect-uri_loopback)
documents this redirect method.

### Google Drive API is disabled or a request returns HTTP 403

Enable **Google Drive API** in the same Google Cloud project that owns the
client ID. Console changes can take a few minutes to propagate; reconnect or
retry after the API is enabled.

### The browser opens but does not return to the launcher

Allow the browser to open the `127.0.0.1` callback. A firewall, security tool,
VPN, or proxy that blocks local loopback traffic can interrupt the desktop
OAuth flow. The launcher waits five minutes before timing out; retry after
allowing local loopback traffic.

### `invalid_grant`, the account was disconnected, or authorization expired

Select **Disconnect**, then **Connect** and approve access again. Google refresh
tokens can stop working after user revocation, Testing expiration, account or
administrator policy changes, and other security events. See Google's
[refresh-token expiration documentation](https://developers.google.com/identity/protocols/oauth2#expiration).

### Secure credential storage is unavailable

The launcher intentionally refuses to save a Google refresh token unless the
operating system's protected credential storage is available. Enable or repair
the platform's credential/keychain service, restart the launcher, and connect
again. There is no plaintext-token fallback.

### Connected, but no backups upload

1. Enable **Cloud backups** in **Instance Settings > General**.
2. Confirm the instance or its backup mod has created a regular file in
   `.minecraft/backups`.
3. Wait at least 60 seconds after the file stops changing.
4. Open **Launcher Settings > Backups** and check provider health and **Backup
   activity** for a queued retry or error.
5. Verify that the Google Drive API remains enabled and the OAuth authorization
   has not expired.

## Planned providers

The following providers are **Coming soon**:

- S3-compatible storage
- OneDrive
- FTP
- FTPS
- SFTP

There are no supported credentials, endpoints, or connection steps for these
providers yet. Do not create provider accounts or secrets specifically for
Industrialis until the launcher marks the provider as available.
