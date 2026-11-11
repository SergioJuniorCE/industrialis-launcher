# Launcher context

## Language

**Instance**:
A game installation the launcher can start, shown as a card in the library.
_Avoid_: profile, pack, game

**Library order**:
The order cards appear in the Instances tab: named groups alphabetically, the ungrouped section last, then each group's saved card order. A collapsed group keeps its place in the order.
_Avoid_: sort order, top

**Launch slot**:
One of nine numbered positions that can start a game. Slot N refers to the Nth instance in library order, not to a specific instance.
_Avoid_: keybind, hotkey

**Launch**:
Starting an instance: saving pending file edits, resolving the account, spawning the game, and opening its console. Both Play and a launch slot do this.
_Avoid_: run, start

## Relationships

- A launch slot refers to a position in library order. Reordering the library changes what a slot launches.
- A running instance can be selected by its slot but cannot be launched twice.

## Flagged ambiguities

- "Keybind" was used for both the position and the key combination. The position is a **launch slot**; the keys are its **shortcut**.
