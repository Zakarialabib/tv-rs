# tv-rs-rules (The Decision Engine)

Declarative automation and rule engine for the `tv-rs` ecosystem. Turns events into decisions.

## Capabilities

- **Conditions**: Multi-clause event matching, count thresholds, value comparisons, and time/schedule checks.
- **Actions**: Show message overlay, send manager notifications, switch scenes, adjust display power or input source.
- **Priorities**: Numeric or level-based rule prioritization (`Low`, `Normal`, `High`, `Emergency`).
- **Cooldowns**: Cooldown windows preventing repetitive action triggers.
- **Schedules**: Time-of-day and day-of-week active execution windows.
- **Scenes**: Named collections of pre-configured display and automation actions.
- **Conflict Resolution**: Policy-driven resolution (e.g., `HighestPriorityWins`, `LatestWins`) when actions compete for target resources.
