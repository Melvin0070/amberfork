# agentdiff

Local, all-Rust developer tool that diffs two AI-agent run trajectories, finds the fork
point, and attributes the regression. Architecture is locked in `design-run-diff-debugger.md`
(hybrid passive+record execution model, 13-crate workspace, explainable semantic move-typed
alignment + counterfactual-causal attribution, embedded Leptos SVG/DOM web UI).

## Design System
Always read `DESIGN.md` before making any visual or UI decisions.
All font choices, colors, spacing, layout, and aesthetic direction are defined there.
The north star is "sameness recedes, divergence glows": color is reserved for divergence
(the fork + divergent path in amber `#FF7A1A`); red/green only inside the content-diff pane.
Render with DOM/SVG (never canvas/wgpu) so text stays selectable and accessible.
Do not deviate without explicit user approval. In QA mode, flag any code that doesn't match DESIGN.md.

## Skill routing

When the user's request matches an available skill, invoke it via the Skill tool. When in doubt, invoke the skill.

Key routing rules:
- Product ideas/brainstorming → invoke /office-hours
- Strategy/scope → invoke /plan-ceo-review
- Architecture → invoke /plan-eng-review
- Design system/plan review → invoke /design-consultation or /plan-design-review
- Full review pipeline → invoke /autoplan
- Bugs/errors → invoke /investigate
- QA/testing site behavior → invoke /qa or /qa-only
- Code review/diff check → invoke /review
- Visual polish → invoke /design-review
- Ship/deploy/PR → invoke /ship or /land-and-deploy
- Save progress → invoke /context-save
- Resume context → invoke /context-restore
- Author a backlog-ready spec/issue → invoke /spec
