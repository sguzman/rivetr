# 2026-10-04: Calendar-only detour and Ephemeris spinoff

## Context

Rivetr was resumed after a period of dormancy and its calendar implementation was reviewed in detail.

The calendar had become the immediate area of interest, while Tasks, Kanban, Dictionary, Contacts, and Map were not current priorities.

For a short period on 2026-10-04, Rivetr was formally reframed in documentation as a calendar-only product. A new `CALENDAR_DIRECTION.md` declared that "Calendar is the product" and demoted the other Rivetr surfaces to dormant status.

## Why that direction was reversed

After reconsideration, this was judged to be unfair to Rivetr's actual identity.

Rivetr was created as the native Rust + egui successor to Rivet, not merely as a calendar extraction.

Rivet had grown into a broad productivity application with:

- Tasks
- Kanban
- Calendar
- Contacts
- Dictionary
- Map

Rivetr inherited that broader product lineage intentionally.

The fact that calendar work was the only active interest in October 2026 did not mean the other product surfaces were mistakes or had ceased to belong to Rivetr.

Rewriting Rivetr's identity around one currently active subsystem would have confused:

- current development priority
- long-term product identity
- implementation ancestry

Those are different things.

## Decision

The calendar-only product was split into a new repository:

- `sguzman/ephemeris`

Ephemeris is the dedicated local-first temporal-information system.

Rivetr remains the broad native successor to Rivet.

The calendar-only directives added to Rivetr on 2026-10-04 were therefore removed and the prior broad README was restored.

## What was preserved

The detour is intentionally documented rather than erased.

It established several useful conclusions:

1. Rivetr's calendar work is substantial enough to seed another product.
2. A rich temporal application wants an event-native model rather than permanent task-backed calendar storage.
3. Taria temporal resources deserve a dedicated interface.
4. Rivetr should not be forced to abandon its broader product identity merely because those other surfaces are dormant right now.
5. Ephemeris can inherit calendar code selectively without inheriting Rivetr's non-calendar obligations.

## Project relationship after the decision

```text
Rivet
  broad Tauri productivity application
        |
        v
Rivetr
  broad native Rust + egui successor
        |
        +----> Ephemeris
                 dedicated temporal-information spinoff
```

## Historical documentation changes

The brief calendar-only reframing was introduced by:

- `32f4de1ee5b103bebd166505cfd52aa438ef342b` - `docs: make calendar the canonical Rivetr product direction`
- `f833213ec90256f8ddac8141fbc516e0110b8aa0` - `docs: refocus Rivetr README on calendar`

Those commits remain part of Git history.

Their directives are no longer current project policy.

This document exists so the reversal is explicit rather than silently pretending the calendar-only phase never happened.
