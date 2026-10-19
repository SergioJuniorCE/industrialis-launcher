# Original launcher protocol fixture

`electron-profile-owner.mjs` is a frozen copy of the cooperative Electron
ownership protocol, captured on 2026-10-07. Original source SHA-256:
`383c1b930f398e4782c5307e411981bdfcf9547ed5a3834ec39a10a567c75ead`.

The process tests prefer the actual Electron source while it exists. After its
removal they use this copy to verify the Rust implementation still interoperates
with the old protocol. Do not update this fixture to accommodate a Rust protocol
change: compatibility needs to be preserved or explicitly versioned.

This is a test fixture, not an embedded JavaScript runtime. Ownership messages
contain only a profile-path hash, activation request, owner type and PID. No
profile data or credentials cross the endpoint. Older released Electron binaries
without this protocol still require installer cutover protection.
