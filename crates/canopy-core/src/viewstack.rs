#![allow(dead_code)]
#![allow(clippy::type_complexity)]

use crate::geom::{Point, Rect};
use crate::viewport::ViewPort;
use crate::{Error, Result};

/// A stack of viewports that manages nested view transformations.
///
/// Invariants:
/// - The stack always contains at least one viewport, enforced by:
///   - `new()` requiring an initial viewport
///   - `pop()` preventing removal of the last item
/// - The first viewport's view represents the physical screen dimensions
///   (i.e., its view size defines the screen size for the entire stack)
pub struct ViewStack {
    views: Vec<ViewPort>,
}

impl ViewStack {
    pub fn new(initial: ViewPort) -> Self {
        Self {
            views: vec![initial],
        }
    }

    pub fn push(&mut self, view: ViewPort) {
        // Ensure the new viewport is positioned within the parent's canvas
        // We know views always has at least one item
        let parent = self.views.last().unwrap();
        let parent_canvas = parent.canvas().rect();

        assert!(
            parent_canvas.contains_point(view.position()),
            "ViewPort position {:?} is outside parent's canvas {:?}",
            view.position(),
            parent_canvas
        );

        // Also check that the child's view rectangle (at its position in parent's canvas)
        // is completely contained within the parent's canvas
        // The actual rectangle occupied by the child in parent's canvas is:
        // position + view's top-left offset, with view's width and height
        let child_rect_in_parent = Rect::new(
            view.position().x + view.view().tl.x,
            view.position().y + view.view().tl.y,
            view.view().w,
            view.view().h,
        );

        assert!(
            parent_canvas.contains_rect(&child_rect_in_parent),
            "ViewPort's view {:?} at position {:?} is not completely contained within parent's canvas {:?}",
            view.view(),
            view.position(),
            parent_canvas
        );

        self.views.push(view);
    }

    pub fn pop(&mut self) -> Result<ViewPort> {
        if self.views.len() <= 1 {
            return Err(Error::Geometry(
                "Cannot pop the last viewport from stack".into(),
            ));
        }
        Ok(self.views.pop().unwrap())
    }

    /// Returns the physical screen dimensions as a rectangle rooted at (0,0).
    ///
    /// The size is determined by the first viewport's view, which by convention
    /// represents the actual screen size. The position of the first viewport itself
    /// is ignored - only its view dimensions matter.
    pub fn root_screen(&self) -> Rect {
        self.views[0].view().at((0, 0))
    }

    /// Calculates the projection from the final viewport's canvas to the screen.
    ///
    /// Returns a tuple of (canvas_rect, screen_rect) where:
    /// - canvas_rect: The region in the final viewport's canvas that we're drawing
    /// - screen_rect: The corresponding region on the screen where it will be displayed
    ///
    /// These rectangles always have the same dimensions but different positions:
    /// canvas_rect is in the final viewport's coordinate system, while screen_rect
    /// is in absolute screen coordinates.
    ///
    /// Returns None if the viewport stack results in no visible area (e.g., when
    /// viewports are positioned outside their parent's visible area).
    pub fn projection(&self) -> Option<(Rect, Rect)> {
        // Start with the first viewport's view as the screen
        // The first viewport's position is ignored since it represents the physical screen
        let mut current_screen = self.views[0].view().at((0, 0));

        // For each subsequent viewport, we need to calculate where its view
        // appears on screen, taking into account its position within the parent
        for i in 1..self.views.len() {
            let viewport = &self.views[i];
            let parent = &self.views[i - 1];

            // Map the viewport's position through the parent to screen
            if let Some(pos_on_screen) = parent.project_point(viewport.position()) {
                // Add the viewport's own view offset
                let view_offset_on_screen = Point {
                    x: pos_on_screen.x + viewport.view().tl.x,
                    y: pos_on_screen.y + viewport.view().tl.y,
                };

                // Create the viewport's screen rectangle
                let viewport_screen = viewport.view().at(view_offset_on_screen);

                // Intersect with current screen area
                current_screen = current_screen.intersect(&viewport_screen)?;
            } else {
                // Position is outside parent's view
                return None;
            }
        }

        let screen_rect = current_screen;

        // Now calculate the canvas rect
        // If there's only one viewport, the canvas rect is just the view rect
        let canvas_rect = if self.views.len() == 1 {
            self.views[0].view()
        } else {
            // We need to work backwards through the viewport stack to find
            // what portion of the final viewport's canvas corresponds to the screen rect

            // Start with the screen rect
            let mut current_rect = screen_rect;

            // For each viewport from first to second-to-last, we need to
            // transform the rect from screen coordinates to canvas coordinates
            for i in 0..self.views.len() - 1 {
                let viewport = &self.views[i];

                // Transform from screen to this viewport's canvas coordinates
                // by subtracting the viewport's screen position
                let screen_pos = viewport.screen_rect().tl;
                current_rect = current_rect.shift(-(screen_pos.x as i16), -(screen_pos.y as i16));

                // Now subtract the position of the next viewport within this canvas
                let next_pos = self.views[i + 1].position();
                current_rect = current_rect.shift(-(next_pos.x as i16), -(next_pos.y as i16));
            }

            // Finally, we're in the last viewport's canvas coordinates
            // We need to account for the last viewport's view offset
            let last_view = self.views.last().unwrap().view();
            current_rect = current_rect.shift(-(last_view.tl.x as i16), -(last_view.tl.y as i16));

            // Add back the view offset to get the actual canvas rect
            current_rect.shift(last_view.tl.x as i16, last_view.tl.y as i16)
        };

        // Verify the invariant that canvas_rect and screen_rect have the same size
        debug_assert_eq!(
            (canvas_rect.w, canvas_rect.h),
            (screen_rect.w, screen_rect.h),
            "canvas_rect and screen_rect must have the same dimensions"
        );

        Some((canvas_rect, screen_rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        // Test new()
        let view1 = ViewPort::new((100, 50), (0, 0, 100, 50), (0, 0)).unwrap();
        let stack = ViewStack::new(view1);
        assert_eq!(stack.views.len(), 1);

        // Test push() and pop()
        let view2 = ViewPort::new((80, 40), (10, 10, 60, 30), (10, 10)).unwrap();
        let view3 = ViewPort::new((60, 30), (5, 5, 50, 20), (15, 15)).unwrap();

        let mut stack = ViewStack::new(view1);
        stack.push(view2);
        stack.push(view3);

        // Pop should return in LIFO order
        assert_eq!(stack.pop().unwrap(), view3);
        assert_eq!(stack.pop().unwrap(), view2);

        // Cannot pop the last item
        let result = stack.pop();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "geometry");
    }

    #[test]
    fn test_root_screen() {
        // First viewport with view (0,0,80,60)
        let view1 = ViewPort::new((100, 100), (0, 0, 80, 60), (10, 10)).unwrap();
        let mut stack = ViewStack::new(view1);

        // root_screen() should return (0,0,80,60)
        assert_eq!(stack.root_screen(), Rect::new(0, 0, 80, 60));

        // Add another viewport - root_screen() should still return first viewport's view size
        let view2 = ViewPort::new((50, 50), (5, 5, 40, 30), (20, 20)).unwrap();
        stack.push(view2);

        assert_eq!(stack.root_screen(), Rect::new(0, 0, 80, 60));
    }

    #[test]
    fn test_screen_rect_single_viewport() {
        struct TestCase {
            name: &'static str,
            viewport: ((u16, u16), (u16, u16, u16, u16), (u16, u16)),
            expected_screen: Rect,
            expected_canvas: Rect,
        }

        let test_cases = vec![
            TestCase {
                name: "Full screen viewport",
                viewport: ((100, 100), (0, 0, 100, 100), (0, 0)),
                expected_screen: Rect::new(0, 0, 100, 100),
                expected_canvas: Rect::new(0, 0, 100, 100),
            },
            TestCase {
                name: "Partial view of larger canvas",
                viewport: ((200, 150), (0, 0, 100, 100), (0, 0)),
                expected_screen: Rect::new(0, 0, 100, 100),
                expected_canvas: Rect::new(0, 0, 100, 100),
            },
            TestCase {
                name: "View with offset into canvas",
                viewport: ((200, 150), (20, 15, 100, 100), (0, 0)),
                expected_screen: Rect::new(0, 0, 100, 100),
                expected_canvas: Rect::new(20, 15, 100, 100),
            },
        ];

        for tc in test_cases {
            let view = ViewPort::new(tc.viewport.0, tc.viewport.1, tc.viewport.2).unwrap();
            let stack = ViewStack::new(view);

            let projection = stack.projection();
            assert_eq!(
                projection,
                Some((tc.expected_canvas, tc.expected_screen)),
                "projection failed for '{}'",
                tc.name
            );

            // Verify that root_screen matches the expected screen size
            let root = stack.root_screen();
            assert_eq!(
                (root.w, root.h),
                (tc.expected_screen.w, tc.expected_screen.h),
                "root_screen size must match screen size for '{}'",
                tc.name
            );
        }
    }

    #[test]
    fn test_first_viewport_position_ignored() {
        // Test that the first viewport's position is ignored since it represents the screen
        let view1_at_origin = ViewPort::new((100, 100), (0, 0, 80, 60), (0, 0)).unwrap();
        let view1_with_position = ViewPort::new((100, 100), (0, 0, 80, 60), (50, 50)).unwrap();

        let stack1 = ViewStack::new(view1_at_origin);
        let stack2 = ViewStack::new(view1_with_position);

        // Both should have the same projection despite different positions
        assert_eq!(stack1.projection(), stack2.projection());
        assert_eq!(stack1.root_screen(), stack2.root_screen());

        // Both should project to screen at (0,0) with size 80x60
        assert_eq!(
            stack1.projection(),
            Some((Rect::new(0, 0, 80, 60), Rect::new(0, 0, 80, 60)))
        );
    }

    #[test]
    fn test_screen_rect_two_viewports() {
        struct TestCase {
            name: &'static str,
            viewport1: ((u16, u16), (u16, u16, u16, u16), (u16, u16)),
            viewport2: ((u16, u16), (u16, u16, u16, u16), (u16, u16)),
            expected_screen: Option<Rect>,
            expected_canvas: Option<Rect>,
        }

        let test_cases = vec![
            TestCase {
                name: "Both viewports full canvas views at origin",
                viewport1: ((10, 10), (0, 0, 10, 10), (0, 0)),
                viewport2: ((10, 10), (0, 0, 10, 10), (0, 0)),
                expected_screen: Some(Rect::new(0, 0, 10, 10)),
                expected_canvas: Some(Rect::new(0, 0, 10, 10)),
            },
            TestCase {
                name: "Second viewport positioned within first",
                viewport1: ((10, 10), (0, 0, 10, 10), (0, 0)),
                viewport2: ((8, 8), (0, 0, 8, 8), (2, 2)),
                expected_screen: Some(Rect::new(2, 2, 8, 8)),
                expected_canvas: Some(Rect::new(0, 0, 8, 8)),
            },
            TestCase {
                name: "Second viewport with partial view",
                viewport1: ((10, 10), (0, 0, 10, 10), (0, 0)),
                viewport2: ((10, 10), (2, 2, 6, 6), (1, 1)),
                expected_screen: Some(Rect::new(3, 3, 6, 6)),
                expected_canvas: Some(Rect::new(2, 2, 6, 6)),
            },
            TestCase {
                name: "Second viewport positioned at edge of first",
                viewport1: ((10, 10), (0, 0, 10, 10), (0, 0)),
                viewport2: ((5, 5), (0, 0, 5, 5), (5, 5)),
                expected_screen: Some(Rect::new(5, 5, 5, 5)),
                expected_canvas: Some(Rect::new(0, 0, 5, 5)),
            },
            TestCase {
                name: "Child with offset view",
                viewport1: ((10, 10), (0, 0, 10, 10), (0, 0)),
                viewport2: ((8, 8), (1, 1, 4, 4), (3, 3)),
                expected_screen: Some(Rect::new(4, 4, 4, 4)),
                expected_canvas: Some(Rect::new(1, 1, 4, 4)),
            },
            TestCase {
                name: "Complex view positioning",
                viewport1: ((10, 10), (0, 0, 10, 10), (0, 0)),
                viewport2: ((6, 6), (1, 1, 3, 3), (2, 2)),
                expected_screen: Some(Rect::new(3, 3, 3, 3)),
                expected_canvas: Some(Rect::new(1, 1, 3, 3)),
            },
        ];

        for tc in test_cases {
            let view1 = ViewPort::new(tc.viewport1.0, tc.viewport1.1, tc.viewport1.2).unwrap();
            let mut stack = ViewStack::new(view1);

            let view2 = ViewPort::new(tc.viewport2.0, tc.viewport2.1, tc.viewport2.2).unwrap();
            stack.push(view2);

            let projection = stack.projection();
            let expected = tc.expected_canvas.zip(tc.expected_screen);
            assert_eq!(projection, expected, "projection failed for '{}'", tc.name);
        }
    }

    #[test]
    #[should_panic(expected = "is outside parent's canvas")]
    fn test_push_constraint_position_outside_parent() {
        // Parent has canvas (100,100)
        let view1 = ViewPort::new((100, 100), (0, 0, 50, 50), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);

        // Child at position (101,101) which is outside parent's canvas (0,0,100,100)
        let view2 = ViewPort::new((50, 50), (0, 0, 30, 30), (101, 101)).unwrap();
        stack.push(view2); // Should panic
    }

    #[test]
    #[should_panic(expected = "is not completely contained within parent's canvas")]
    fn test_push_constraint_view_not_contained() {
        // Parent has canvas (100,100)
        let view1 = ViewPort::new((100, 100), (0, 0, 50, 50), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);

        // Child's view starts at (0,0) in its own canvas and has size 30x30
        // Position the child at (80,80) in parent's canvas
        // This means the actual view rectangle would be at (80,80) to (110,110)
        // which extends beyond the parent's canvas
        let view2 = ViewPort::new((50, 50), (0, 0, 30, 30), (80, 80)).unwrap();
        stack.push(view2); // Should panic
    }

    #[test]
    fn test_push_constraint_valid() {
        // Parent has canvas (100,100)
        let view1 = ViewPort::new((100, 100), (0, 0, 50, 50), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);

        // Child fits within parent's canvas
        let view2 = ViewPort::new((80, 80), (0, 0, 60, 60), (20, 20)).unwrap();
        stack.push(view2); // Should not panic (60x60 at (20,20) ends at (80,80))

        // Edge case: child exactly at parent's canvas boundary (view2 has 80x80 canvas)
        let view3 = ViewPort::new((40, 40), (0, 0, 40, 40), (40, 40)).unwrap();
        stack.push(view3); // Should not panic (40x40 at (40,40) ends at (80,80))
    }

    #[test]
    fn test_screen_rect_three_viewports() {
        // Complex three viewport test
        let view1 = ViewPort::new((200, 200), (0, 0, 100, 100), (0, 0)).unwrap();
        let view2 = ViewPort::new((150, 150), (0, 0, 80, 80), (10, 10)).unwrap();
        let view3 = ViewPort::new((100, 100), (0, 0, 60, 60), (20, 20)).unwrap();

        let mut stack = ViewStack::new(view1);
        stack.push(view2);
        stack.push(view3);

        // View3 is at position (20,20) relative to view2, which is at (10,10)
        // So view3's screen rect is (10+20, 10+20, 60, 60) = (30,30,60,60)
        let projection = stack.projection();
        assert_eq!(
            projection,
            Some((Rect::new(0, 0, 60, 60), Rect::new(30, 30, 60, 60)))
        );
    }

    #[test]
    fn test_screen_canvas_rect_size_invariant() {
        // Test that screen_rect and canvas_rect always have the same dimensions

        // Single viewport
        let view1 = ViewPort::new((100, 100), (10, 10, 50, 50), (5, 5)).unwrap();
        let stack = ViewStack::new(view1);
        let (canvas, screen) = stack.projection().unwrap();
        assert_eq!((screen.w, screen.h), (canvas.w, canvas.h));

        // Two viewports with various configurations
        let test_cases = vec![
            // Simple nested viewport
            (
                ViewPort::new((50, 50), (0, 0, 50, 50), (0, 0)).unwrap(),
                ViewPort::new((40, 40), (5, 5, 30, 30), (10, 10)).unwrap(),
            ),
            // Viewport with offset views
            (
                ViewPort::new((100, 100), (20, 20, 60, 60), (0, 0)).unwrap(),
                ViewPort::new((50, 50), (10, 10, 20, 20), (25, 25)).unwrap(),
            ),
            // Edge-aligned viewports
            (
                ViewPort::new((20, 20), (0, 0, 20, 20), (0, 0)).unwrap(),
                ViewPort::new((10, 10), (0, 0, 10, 10), (10, 10)).unwrap(),
            ),
        ];

        for (view1, view2) in test_cases {
            let mut stack = ViewStack::new(view1);
            stack.push(view2);

            if let Some((canvas, screen)) = stack.projection() {
                assert_eq!(
                    (screen.w, screen.h),
                    (canvas.w, canvas.h),
                    "screen_rect {screen:?} and canvas_rect {canvas:?} must have same dimensions"
                );
            }
        }

        // Three viewports
        let view1 = ViewPort::new((100, 100), (0, 0, 100, 100), (0, 0)).unwrap();
        let view2 = ViewPort::new((80, 80), (10, 10, 60, 60), (20, 20)).unwrap();
        let view3 = ViewPort::new((40, 40), (5, 5, 30, 30), (15, 15)).unwrap();

        let mut stack = ViewStack::new(view1);
        stack.push(view2);
        stack.push(view3);

        let (canvas, screen) = stack.projection().unwrap();
        assert_eq!((screen.w, screen.h), (canvas.w, canvas.h));
    }
}
