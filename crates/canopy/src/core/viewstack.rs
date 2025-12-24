#![allow(dead_code)]
#![allow(clippy::type_complexity)]

use super::viewport::ViewPort;
use crate::{
    error::{Error, Result},
    geom::Rect,
};

/// A stack of viewports that manages nested view transformations.
pub struct ViewStack {
    /// Stored viewport stack.
    views: Vec<ViewPort>,
}

impl ViewStack {
    /// Construct a stack with an initial viewport.
    pub fn new(initial: ViewPort) -> Self {
        Self {
            views: vec![initial],
        }
    }

    /// Push a viewport onto the stack.
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

        // Check that the child's actual placement in the parent overlaps with parent's canvas
        // The child occupies a rectangle at its position with size equal to its view size
        let child_placement_in_parent = Rect::new(
            view.position().x,
            view.position().y,
            view.view().w,
            view.view().h,
        );

        assert!(
            parent_canvas
                .intersect(&child_placement_in_parent)
                .is_some(),
            "ViewPort at position {:?} with size {}x{} does not overlap with parent's canvas {:?}",
            view.position(),
            view.view().w,
            view.view().h,
            parent_canvas
        );

        self.views.push(view);
    }

    /// Pop the top viewport, keeping at least one in the stack.
    pub fn pop(&mut self) -> Result<ViewPort> {
        if self.views.len() <= 1 {
            return Err(Error::Geometry(
                "Cannot pop the last viewport from stack".into(),
            ));
        }
        Ok(self.views.pop().unwrap())
    }

    /// Get the top viewport on the stack
    pub fn top(&self) -> &ViewPort {
        self.views.last().unwrap()
    }

    /// Returns the physical screen dimensions as a rectangle rooted at (0,0).
    ///
    /// The size is determined by the first viewport's view, which by convention
    /// represents the actual screen size. The position of the first viewport itself
    /// is ignored - only its view dimensions matter.
    pub fn root_screen(&self) -> Rect {
        let v = self.views[0].view();
        Rect::new(0, 0, v.w, v.h)
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
        let first_view = self.views[0].view();
        let mut screen_clip = Rect::new(0, 0, first_view.w, first_view.h);
        let mut view_screen_origin_x = 0i32;
        let mut view_screen_origin_y = 0i32;

        for i in 1..self.views.len() {
            let viewport = &self.views[i];
            let parent = &self.views[i - 1];

            // Update the screen origin for the current viewport
            // New Origin = Old Origin + (Child Pos - Parent View TL)
            view_screen_origin_x += viewport.position().x as i32 - parent.view().tl.x as i32;
            view_screen_origin_y += viewport.position().y as i32 - parent.view().tl.y as i32;

            // Calculate the screen rect of the current viewport's view, handling negative coordinates
            // by clamping to 0 and reducing dimensions accordingly.
            let view_x = view_screen_origin_x;
            let view_y = view_screen_origin_y;
            let view_w = viewport.view().w as i32;
            let view_h = viewport.view().h as i32;

            // Effective start positions (clamped to 0)
            let eff_x = view_x.max(0);
            let eff_y = view_y.max(0);

            // Effective dimensions
            let eff_w = view_w - (eff_x - view_x);
            let eff_h = view_h - (eff_y - view_y);

            if eff_w <= 0 || eff_h <= 0 {
                return None;
            }

            let viewport_screen_rect =
                Rect::new(eff_x as u32, eff_y as u32, eff_w as u32, eff_h as u32);

            // Intersect with the accumulated clip
            screen_clip = screen_clip.intersect(&viewport_screen_rect)?;
        }

        let screen_rect = screen_clip;

        // Now calculate the canvas rect by tracking through the viewport transformations
        let canvas_rect = if self.views.len() == 1 {
            self.views[0].view()
        } else {
            // Track the visible region through each viewport transformation
            let mut visible_in_parent = self.views[0].view();

            for i in 1..self.views.len() {
                let viewport = &self.views[i];
                let _parent = &self.views[i - 1];

                // Calculate where the child appears in parent's canvas
                let child_rect_in_parent = Rect::new(
                    viewport.position().x,
                    viewport.position().y,
                    viewport.view().w,
                    viewport.view().h,
                );

                // Find the intersection with what's visible in the parent
                if let Some(visible_part) = visible_in_parent.intersect(&child_rect_in_parent) {
                    // Transform this visible part to child's canvas coordinates
                    // by shifting back by the child's position
                    visible_in_parent = Rect {
                        tl: visible_part
                            .tl
                            .scroll(
                                -(viewport.position().x as i32),
                                -(viewport.position().y as i32),
                            )
                            .scroll(viewport.view().tl.x as i32, viewport.view().tl.y as i32),
                        w: visible_part.w,
                        h: visible_part.h,
                    };
                } else {
                    // No intersection - nothing visible
                    return None;
                }
            }

            // The final visible_in_parent is our canvas rect
            visible_in_parent
        };

        // Verify the invariant that canvas_rect and screen_rect have the same size
        debug_assert_eq!(
            (canvas_rect.w, canvas_rect.h),
            (screen_rect.w, screen_rect.h),
            "canvas_rect and screen_rect must have the same dimensions - canvas: {canvas_rect:?}, screen: {screen_rect:?}"
        );

        Some((canvas_rect, screen_rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCase {
        name: &'static str,
        viewports: Vec<((u32, u32), (u32, u32, u32, u32), (u32, u32))>,
        projections: Vec<Option<((u32, u32, u32, u32), (u32, u32, u32, u32))>>,
    }

    impl TestCase {
        fn run(&self) {
            assert!(
                !self.viewports.is_empty(),
                "Test case must have at least one viewport"
            );
            assert_eq!(
                self.viewports.len(),
                self.projections.len(),
                "Number of viewports must match number of projections for '{}'",
                self.name
            );

            let first = &self.viewports[0];
            let view = ViewPort::new(first.0, first.1, first.2).unwrap();
            let mut stack = ViewStack::new(view);

            // Check projection after first viewport
            let projection = stack.projection();
            let expected = self.projections[0].map(|(canvas, screen)| {
                (
                    Rect::new(canvas.0, canvas.1, canvas.2, canvas.3),
                    Rect::new(screen.0, screen.1, screen.2, screen.3),
                )
            });
            assert_eq!(
                projection, expected,
                "projection failed for '{}' after viewport 0",
                self.name
            );

            // For single viewport tests, verify root_screen
            if self.viewports.len() == 1 && self.projections[0].is_some() {
                let root = stack.root_screen();
                let (_, expected_screen) = self.projections[0].unwrap();
                assert_eq!(
                    (root.w, root.h),
                    (expected_screen.2, expected_screen.3),
                    "root_screen size must match screen size for '{}'",
                    self.name
                );
            }

            // Push remaining viewports and check projections
            for (i, viewport) in self.viewports[1..].iter().enumerate() {
                let view = ViewPort::new(viewport.0, viewport.1, viewport.2).unwrap();
                stack.push(view);

                let projection = stack.projection();
                let expected = self.projections[i + 1].map(|(canvas, screen)| {
                    (
                        Rect::new(canvas.0, canvas.1, canvas.2, canvas.3),
                        Rect::new(screen.0, screen.1, screen.2, screen.3),
                    )
                });
                assert_eq!(
                    projection,
                    expected,
                    "projection failed for '{}' after viewport {}",
                    self.name,
                    i + 1
                );
            }
        }
    }

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
        assert_eq!(
            result.unwrap_err().to_string(),
            "geometry: Cannot pop the last viewport from stack"
        );
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
    fn test_projections() {
        let test_cases = vec![
            // Single viewport tests
            TestCase {
                name: "Full viewport of canvas",
                viewports: vec![((100, 100), (0, 0, 100, 100), (0, 0))],
                projections: vec![Some(((0, 0, 100, 100), (0, 0, 100, 100)))],
            },
            TestCase {
                name: "Viewport with offset view into canvas",
                viewports: vec![((200, 150), (20, 15, 100, 100), (0, 0))],
                projections: vec![Some(((20, 15, 100, 100), (0, 0, 100, 100)))],
            },
            // Two viewport tests that can be extended to three
            TestCase {
                name: "Nested viewports - full canvas views",
                viewports: vec![
                    ((100, 100), (0, 0, 100, 100), (0, 0)),
                    ((80, 80), (0, 0, 80, 80), (10, 10)),
                    ((60, 60), (0, 0, 60, 60), (10, 10)),
                ],
                projections: vec![
                    Some(((0, 0, 100, 100), (0, 0, 100, 100))),
                    Some(((0, 0, 80, 80), (10, 10, 80, 80))),
                    Some(((0, 0, 60, 60), (20, 20, 60, 60))),
                ],
            },
            TestCase {
                name: "Child partially clipped",
                viewports: vec![
                    ((20, 20), (0, 0, 20, 20), (0, 0)),
                    ((15, 15), (0, 0, 15, 15), (10, 10)),
                    ((10, 10), (0, 0, 10, 10), (5, 5)),
                ],
                projections: vec![
                    Some(((0, 0, 20, 20), (0, 0, 20, 20))),
                    Some(((0, 0, 10, 10), (10, 10, 10, 10))),
                    Some(((0, 0, 5, 5), (15, 15, 5, 5))),
                ],
            },
            // Clipping edge cases
            TestCase {
                name: "Second viewport completely outside parent view",
                viewports: vec![
                    ((50, 50), (40, 40, 10, 10), (0, 0)),
                    ((30, 30), (0, 0, 30, 30), (5, 5)),
                    ((20, 20), (0, 0, 20, 20), (5, 5)),
                ],
                projections: vec![Some(((40, 40, 10, 10), (0, 0, 10, 10))), None, None],
            },
            TestCase {
                name: "Viewport positioned at parent view edge",
                viewports: vec![
                    ((20, 20), (0, 0, 20, 20), (0, 0)),
                    ((15, 15), (0, 0, 15, 15), (19, 19)),
                    ((10, 10), (0, 0, 10, 10), (0, 0)),
                ],
                projections: vec![
                    Some(((0, 0, 20, 20), (0, 0, 20, 20))),
                    Some(((0, 0, 1, 1), (19, 19, 1, 1))),
                    Some(((0, 0, 1, 1), (19, 19, 1, 1))),
                ],
            },
            TestCase {
                name: "Progressive clipping with offset views",
                viewports: vec![
                    ((50, 50), (0, 0, 50, 50), (0, 0)),
                    ((40, 40), (0, 0, 30, 30), (10, 10)),
                    ((25, 25), (0, 0, 20, 20), (5, 5)),
                ],
                projections: vec![
                    Some(((0, 0, 50, 50), (0, 0, 50, 50))),
                    Some(((0, 0, 30, 30), (10, 10, 30, 30))),
                    Some(((0, 0, 20, 20), (15, 15, 20, 20))),
                ],
            },
            TestCase {
                name: "Third viewport clips to tiny area",
                viewports: vec![
                    ((30, 30), (0, 0, 30, 30), (0, 0)),
                    ((25, 25), (0, 0, 25, 25), (5, 5)),
                    ((20, 20), (0, 0, 20, 20), (24, 24)),
                ],
                projections: vec![
                    Some(((0, 0, 30, 30), (0, 0, 30, 30))),
                    Some(((0, 0, 25, 25), (5, 5, 25, 25))),
                    Some(((0, 0, 1, 1), (29, 29, 1, 1))),
                ],
            },
            TestCase {
                name: "Complex three-layer clipping",
                viewports: vec![
                    ((100, 100), (10, 10, 80, 80), (0, 0)),
                    ((90, 90), (5, 5, 60, 60), (15, 15)),
                    ((80, 80), (10, 10, 40, 40), (10, 10)),
                ],
                projections: vec![
                    Some(((10, 10, 80, 80), (0, 0, 80, 80))),
                    Some(((5, 5, 60, 60), (5, 5, 60, 60))),
                    Some(((10, 10, 40, 40), (10, 10, 40, 40))),
                ],
            },
            TestCase {
                name: "Viewport chain with progressive clipping",
                viewports: vec![
                    ((40, 40), (0, 0, 40, 40), (0, 0)),
                    ((35, 35), (5, 5, 25, 25), (5, 5)),
                    ((30, 30), (5, 5, 15, 15), (5, 5)),
                ],
                projections: vec![
                    Some(((0, 0, 40, 40), (0, 0, 40, 40))),
                    Some(((5, 5, 25, 25), (5, 5, 25, 25))),
                    Some(((5, 5, 15, 15), (5, 5, 15, 15))),
                ],
            },
            TestCase {
                name: "Multiple viewports with extreme edge positioning",
                viewports: vec![
                    ((50, 50), (0, 0, 50, 50), (0, 0)),
                    ((40, 40), (0, 0, 40, 40), (49, 0)),
                    ((30, 30), (0, 0, 30, 30), (0, 39)),
                ],
                projections: vec![
                    Some(((0, 0, 50, 50), (0, 0, 50, 50))),
                    Some(((0, 0, 1, 40), (49, 0, 1, 40))),
                    Some(((0, 0, 1, 1), (49, 39, 1, 1))),
                ],
            },
        ];

        for tc in test_cases {
            tc.run();
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
    fn test_viewport_clipping() {
        // Test that viewports are properly clipped when they extend beyond parent bounds
        let view1 = ViewPort::new((20, 20), (0, 0, 20, 20), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);

        // Child viewport extends beyond parent - should be clipped, not rejected
        let view2 = ViewPort::new((30, 30), (0, 0, 30, 30), (10, 10)).unwrap();
        stack.push(view2); // Should not panic - partial overlap is allowed

        // Projection should show clipped result
        let projection = stack.projection();
        assert_eq!(
            projection,
            Some((Rect::new(0, 0, 10, 10), Rect::new(10, 10, 10, 10))),
            "Viewport should be clipped to parent bounds"
        );
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

    #[test]
    fn test_viewport_partial_visibility_corners() {
        // Test partial visibility from different corners

        // Parent view shows only bottom-right quadrant of its canvas
        let view1 = ViewPort::new((20, 20), (10, 10, 10, 10), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);

        // Child positioned so its view overlaps with parent's visible area
        let view2 = ViewPort::new((15, 15), (0, 0, 15, 15), (5, 5)).unwrap();
        stack.push(view2);
        assert_eq!(
            stack.projection(),
            Some((Rect::new(5, 5, 10, 10), Rect::new(0, 0, 10, 10))),
            "Child visible from (5,5) in its canvas"
        );

        // Test with parent showing only top-left quadrant
        let view1 = ViewPort::new((20, 20), (0, 0, 10, 10), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);

        // Child positioned to show only its bottom-right corner
        let view2 = ViewPort::new((15, 15), (0, 0, 15, 15), (5, 5)).unwrap();
        stack.push(view2);
        assert_eq!(
            stack.projection(),
            Some((Rect::new(0, 0, 5, 5), Rect::new(5, 5, 5, 5))),
            "Only bottom-right corner of child should be visible"
        );

        // Parent shows middle section, child overlaps partially on all sides
        let view1 = ViewPort::new((30, 30), (10, 10, 10, 10), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);

        let view2 = ViewPort::new((20, 20), (0, 0, 20, 20), (5, 5)).unwrap();
        stack.push(view2);
        assert_eq!(
            stack.projection(),
            Some((Rect::new(5, 5, 10, 10), Rect::new(0, 0, 10, 10))),
            "Child should be clipped to parent's view"
        );
    }

    #[test]
    fn test_viewport_corner_clipping() {
        // Test specific corner clipping scenarios

        // Bottom-right corner: Parent at origin, child extends beyond
        let view1 = ViewPort::new((10, 10), (0, 0, 10, 10), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);
        let view2 = ViewPort::new((8, 8), (0, 0, 8, 8), (5, 5)).unwrap();
        stack.push(view2);
        assert_eq!(
            stack.projection(),
            Some((Rect::new(0, 0, 5, 5), Rect::new(5, 5, 5, 5))),
            "Bottom-right 5x5 corner visible"
        );

        // Top-left corner: Child partially before parent's view
        let view1 = ViewPort::new((20, 20), (5, 5, 10, 10), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);
        let view2 = ViewPort::new((10, 10), (0, 0, 10, 10), (0, 0)).unwrap();
        stack.push(view2);
        assert_eq!(
            stack.projection(),
            Some((Rect::new(5, 5, 5, 5), Rect::new(0, 0, 5, 5))),
            "Top-left 5x5 corner visible"
        );

        // Top-right corner: Mix of horizontal and vertical clipping
        let view1 = ViewPort::new((20, 20), (0, 5, 10, 10), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);
        let view2 = ViewPort::new((10, 10), (0, 0, 10, 10), (0, 0)).unwrap();
        stack.push(view2);
        assert_eq!(
            stack.projection(),
            Some((Rect::new(0, 5, 10, 5), Rect::new(0, 0, 10, 5))),
            "Top portion visible"
        );

        // Bottom-left corner: Another mix
        let view1 = ViewPort::new((20, 20), (5, 0, 10, 10), (0, 0)).unwrap();
        let mut stack = ViewStack::new(view1);
        let view2 = ViewPort::new((10, 10), (0, 0, 10, 10), (0, 0)).unwrap();
        stack.push(view2);
        assert_eq!(
            stack.projection(),
            Some((Rect::new(5, 0, 5, 10), Rect::new(0, 0, 5, 10))),
            "Left portion visible"
        );
    }
}
