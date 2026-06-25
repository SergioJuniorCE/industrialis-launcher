# Taste (Continuously Learned by [CommandCode][cmd])

[cmd]: https://commandcode.ai/

# dev
- User keeps the dev server running. Do not start, restart, or kill it unless explicitly asked. Assume hot reload is active. Confidence: 0.95

# workspace
- After background shell/agent work, delete the project-root `terminals/` folder if it was created. It is Cursor agent output, not part of the repo. Confidence: 0.95

# frontend
- Use shadcn/ui for React component library. Confidence: 0.65
- Show only installed instances, not all possible Minecraft versions. Provide a "create instance" button that lets users select and install a specific version. Confidence: 0.75
- Use a 3-panel layout with top toolbar (not sidebar) for navigation: Add Instance, Instances/Settings/Accounts toggles, instance list on left, tabbed details panel on right, status bar on bottom. Confidence: 0.60

# code
- Never mention Prism Launcher (or other third-party launchers) in source code, comments, error strings, or user-facing copy in the repo. Confidence: 0.95

