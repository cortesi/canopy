# Widget Extraction Plan

## Current Status

A `widgets` crate has been created at `/widgets/` as requested, but it cannot be properly implemented without breaking the existing interface due to Rust's restriction on circular dependencies.

## The Problem

- Widgets need types from canopy (Node, StatefulNode, Context, etc.)
- Canopy needs to re-export widgets to maintain the same interface
- This creates a circular dependency: canopy → widgets → canopy

## Prepared Structure

The following has been set up:
- `/widgets/` directory with Cargo.toml
- Placeholder src/lib.rs explaining the situation
- Widgets remain in `canopy/src/widgets/` for now

## Future Solution

To properly extract widgets while keeping the interface the same:

1. **Phase 1**: Extract core types
   - Create `canopy-core` crate with Node, StatefulNode, Context, etc.
   - Update canopy to depend on and re-export canopy-core

2. **Phase 2**: Move widgets
   - Update widgets crate to depend on canopy-core
   - Move widget implementations from canopy to widgets crate
   - Update canopy to depend on and re-export widgets

3. **Phase 3**: Update users
   - No changes needed for users - `canopy::widgets::*` will still work

## Alternative (Breaking Change)

If breaking changes are acceptable:
- Move widgets to separate crate immediately  
- Users would need to add `canopy-widgets` to their dependencies
- Change imports from `canopy::widgets::*` to `canopy_widgets::*`