# Taste (Continuously Learned by [CommandCode][cmd])

[cmd]: https://commandcode.ai/

# dev
- User keeps the dev server running. Do not start, restart, or kill it unless explicitly asked. Assume hot reload is active. Confidence: 0.95
- Preferred git workflow: create a branch, open a PR, review the code, then merge. User frequently requests this exact sequence ("make branch, pr, review, merge"). Confidence: 0.9
- Always do a code review before merging — user expects PRs to be reviewed before they are merged. Confidence: 0.85
- User's repositories use `master` as the default branch name, not `main`. Confidence: 0.8
- User wants all compiler warnings and errors fixed, not just errors. Keep the codebase warning-free. Confidence: 0.8

# workspace
- After background shell/agent work, delete the project-root `terminals/` folder if it was created. It is Cursor agent output, not part of the repo. Confidence: 0.95

# frontend
- Use shadcn/ui for React component library. Confidence: 0.65
- Show only installed instances, not all possible Minecraft versions. Provide a "create instance" button that lets users select and install a specific version. Confidence: 0.75
- Use a 3-panel layout with top toolbar (not sidebar) for navigation: Add Instance, Instances/Settings/Accounts toggles, instance list on left, tabbed details panel on right, status bar on bottom. Confidence: 0.60

# code
- Never mention Prism Launcher (or other third-party launchers) in source code, comments, error strings, or user-facing copy in the repo. Confidence: 0.95

# communication
- User is bilingual (English/Spanish) and may switch to Spanish mid-conversation. Respond in whichever language the user is currently using. Confidence: 0.7
- When diagnosing project issues, user prefers the assistant to execute the relevant command and investigate the output directly rather than only ask them to provide error details. Confidence: 0.9

# ux
- Prefer global/default settings over per-instance configuration when both are possible (e.g., default Java version should be settable globally, not just per-instance). Confidence: 0.7

