# Rivetr Calendar Direction

Status: canonical product direction for the current development phase.

## Product definition

> **Rivetr Calendar is a local-first temporal information system in which events are canonical data and calendars are programmable views.**

Rivetr began as a native Rust + egui successor to the older Rivet productivity application. That inheritance is useful, but it no longer defines the current product scope.

The current product is **the calendar**.

Rivetr should become a fast, deep desktop interface for consuming, inspecting, filtering, combining, visualizing, and eventually editing the large temporal corpus produced by Taria and its temporal resources. It is not currently being developed as a general productivity suite.

## Current development rule

Until this direction is explicitly changed:

- Calendar work is active.
- Calendar infrastructure and Taria temporal-resource integration are active.
- Task infrastructure may remain when it supports calendar semantics, compatibility, deadlines, or projections.
- Tasks as a standalone workspace are dormant.
- Kanban is dormant.
- Dictionary is dormant.
- Contacts are dormant.
- Map is dormant.
- Dormant workspaces should be hidden from the normal UI rather than compete with the calendar for attention.
- Existing dormant code should be preserved unless it materially obstructs the calendar architecture.
- No effort should be spent reaching old Rivet feature parity outside the calendar.

This is a scope decision, not a deletion decision. Dormant features may be resurrected later.

## Why the calendar is different

Rivetr is not primarily an appointment grid and should not inherit the conventional assumption that a calendar container owns an event's organization, visibility, and color.

The intended model is:

1. Events are canonical data.
2. Source representations and provenance are retained.
3. Organization is metadata and ontology.
4. Visibility is a query.
5. A saved calendar is primarily a programmable view over the corpus.
6. Filtering, grouping, sorting, coloring, and rendering are independent operations.
7. External formats and services are imports, synchronization surfaces, or projections; they do not define the canonical model.

The calendar must remain useful for ordinary personal scheduling, but the architecture is optimized for a much denser corpus: politics, government, economics, finance, business, sports, holidays, institutions, research material, personal commitments, and other temporal resources.

## Taria relationship

Taria is expected to provide rich temporal resources rather than merely flat appointment files.

Rivetr must therefore be able to preserve and expose, as the Taria data model matures:

- event identity and stable identifiers
- normalized and raw titles
- source and provenance
- source kind and authority
- domain and category
- geography and jurisdiction
- institution and participants
- event type
- lifecycle/status
- confidence and uncertainty
- source timezone and normalized time
- recurrence and occurrence identity
- relations, collections, and sequences
- snapshot/history membership
- custom/extensible properties
- user annotations that survive source refreshes

Rivetr should consume Taria temporal data without flattening it down to the lowest-common-denominator ICS model.

## Architectural principles

### Local first

Rendering, searching, filtering, navigation, saved views, annotations, and inspection must work without a network connection. Network access is for acquiring and refreshing sources.

### Near-zero interaction latency

Changing date ranges, filters, saved views, colors, density, and inspectors should feel immediate against large local corpora. Avoid network round trips in interactive paths.

### One corpus, many views

An event should not need to be duplicated because it appears in several conceptual calendars. Saved views are queries and presentation configuration over a canonical corpus.

### Dense-data first

The interface must remain useful with dozens or hundreds of events on a day. Compact modes, aggregation, counts, tables, timelines, and zoom-dependent detail are first-class requirements.

### Provenance first

An official native ICS event, an event extracted from an official webpage, an API record, a projected event, and a manual event are not equivalent. The UI and model must retain those distinctions.

### Serious time semantics

Recurrence, all-day dates, floating times, timezone conversion, DST, reschedules, cancellations, and source-vs-display timezone must be modeled deliberately.

### Keyboard first

Common calendar operations should be efficient without a mouse: search, date jump, view switching, selection, filtering, inspection, and bulk actions.

### Progressive complexity

The default calendar should remain immediately understandable. Query algebra, ontology, provenance, snapshots, and advanced visualization should appear when needed rather than overwhelm every screen.

## Current milestone: Calendar Core

The immediate goal is not to implement every long-term calendar feature. It is to turn the existing Rivetr calendar into a trustworthy foundation for the richer system.

Work in this milestone should converge on:

1. **Calendar-only application surface**
   - launch into Calendar
   - hide dormant workspace tabs from normal navigation
   - remove calendar dependence on unrelated UI flows where practical

2. **Explicit calendar data boundary**
   - separate canonical temporal events from task-specific representation
   - identify which current task-backed calendar assumptions are compatibility scaffolding
   - define a migration path that does not destroy existing data

3. **Taria ingestion path**
   - consume Taria temporal resources locally
   - preserve source/provenance and rich metadata
   - support repeatable refresh/update rather than one-off destructive imports

4. **Rich event inspection**
   - expose the actual event structure and provenance
   - make debugging imported/normalized data possible from the UI

5. **Fast query/filter foundation**
   - simple facet filtering first
   - architecture capable of richer boolean/range/query expressions
   - no requirement to reorganize physical calendars to change visibility

6. **Saved view foundation**
   - a view should be able to retain date behavior, filtering, presentation, and eventually grouping/sorting/color rules

7. **Dense calendar rendering**
   - preserve existing year/quarter/month/week/day work
   - improve behavior under large event counts rather than optimizing only for sparse personal appointments

8. **Refresh and source state**
   - clearly expose imported source identity and freshness
   - establish the foundation for local files, snapshots, generated projections, and later remote synchronization

9. **Correctness and performance**
   - tests for ingestion, identity, update/delete behavior, time semantics, and large corpora
   - keep interaction latency low enough that the local database feels instantaneous

## Explicit non-goals for this phase

The following are not reasons to delay Calendar Core:

- restoring the Tasks workspace as a product surface
- expanding Kanban
- finishing Contacts
- finishing Map
- expanding Dictionary
- reproducing every old Rivet setting
- reviving the Tauri/React architecture
- full Taskwarrior CLI parity
- cloud-first synchronization
- Google Calendar feature parity

These may matter later. They do not matter now.

## Long-term calendar capabilities

The longer-term direction includes:

- arbitrary boolean filtering and calendar algebra
- independent grouping, sorting, filtering, and coloring
- first-class saved views and overlays
- month/week/day plus agenda, table, timeline, year, multi-year, heatmap, grouped, and interval views
- recurrence and exception semantics
- serious timezone handling
- history, snapshots, and diffs
- source health and rollover
- event relations, collections, and sequences
- personal annotations
- duplicate/entity resolution
- table/pivot/summary analysis
- reminders defined by queries/views
- bulk operations
- deep links, undo/redo, and audit history
- import from ICS/webcal, CalDAV, JSCalendar/jCal, CSV, JSON, and Taria-native representations
- export as projections rather than canonical storage
- ordinary personal scheduling without reducing public/research temporal data to appointment semantics

## Decision test

When evaluating a feature or architectural choice, ask:

> Does this make Rivetr better at locally interrogating and visualizing a rich temporal corpus?

If not, it is probably not current-priority work.

A second test is:

> Does changing how events are displayed require duplicating or reorganizing the underlying events?

If yes, the design is probably wrong.
