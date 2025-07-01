#![allow(dead_code)]

use crate::geom::{Point, Rect};
use crate::viewport::ViewPort;
use crate::{Error, Result};

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

    /// Returns a rectangle rooted at (0,0) with the same size as the view of the first
    /// item on the stack. This represents the base screen dimensions.
    pub fn root_screen(&self) -> Rect {
        self.views[0].view().at((0, 0))
    }

    /// Returns the rectangle on the screen we are drawing to, after all stacked views
    /// have been taken into account. This progressively narrows down the drawable area
    /// based on each viewport's position within its parent's canvas and its view.
    pub fn screen_rect(&self) -> Option<Rect> {
        // Start with the first viewport's screen rect
        let mut current_screen = self.views[0].screen_rect();

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
                let viewport_screen = Rect::new(
                    view_offset_on_screen.x,
                    view_offset_on_screen.y,
                    viewport.view().w,
                    viewport.view().h,
                );

                // Intersect with current screen area
                current_screen = current_screen.intersect(&viewport_screen)?;
            } else {
                // Position is outside parent's view
                return None;
            }
        }

        Some(current_screen)
    }

    /// Returns the rectangle in the canvas of the final view on the stack that we are
    /// drawing. This rectangle will always be the same size as screen_rect().
    pub fn canvas_rect(&self) -> Option<Rect> {
        // Get the screen rect - this is our final drawable area
        let screen_rect = self.screen_rect()?;

        // If there's only one viewport, the canvas rect is just the view rect
        if self.views.len() == 1 {
            return Some(self.views[0].view());
        }

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
            current_rect = Rect::new(
                current_rect.tl.x.saturating_sub(screen_pos.x),
                current_rect.tl.y.saturating_sub(screen_pos.y),
                current_rect.w,
                current_rect.h,
            );

            // Now subtract the position of the next viewport within this canvas
            if i + 1 < self.views.len() {
                let next_pos = self.views[i + 1].position();
                current_rect = Rect::new(
                    current_rect.tl.x.saturating_sub(next_pos.x),
                    current_rect.tl.y.saturating_sub(next_pos.y),
                    current_rect.w,
                    current_rect.h,
                );
            }
        }

        // Finally, we're in the last viewport's canvas coordinates
        // We need to account for the last viewport's view offset
        let last_view = self.views.last().unwrap().view();
        current_rect = Rect::new(
            current_rect.tl.x.saturating_sub(last_view.tl.x),
            current_rect.tl.y.saturating_sub(last_view.tl.y),
            current_rect.w,
            current_rect.h,
        );

        // Add back the view offset to get the actual canvas rect
        Some(Rect::new(
            last_view.tl.x + current_rect.tl.x,
            last_view.tl.y + current_rect.tl.y,
            current_rect.w,
            current_rect.h,
        ))
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
                name: "Simple viewport at origin",
                viewport: ((100, 100), (0, 0, 50, 30), (0, 0)),
                expected_screen: Rect::new(0, 0, 50, 30),
                expected_canvas: Rect::new(0, 0, 50, 30),
            },
            TestCase {
                name: "Viewport with position offset",
                viewport: ((100, 100), (0, 0, 50, 30), (10, 10)),
                expected_screen: Rect::new(10, 10, 50, 30),
                expected_canvas: Rect::new(0, 0, 50, 30),
            },
            TestCase {
                name: "Viewport with view offset",
                viewport: ((100, 100), (20, 15, 50, 30), (10, 10)),
                expected_screen: Rect::new(10, 10, 50, 30),
                expected_canvas: Rect::new(20, 15, 50, 30),
            },
        ];

        for tc in test_cases {
            let view = ViewPort::new(tc.viewport.0, tc.viewport.1, tc.viewport.2).unwrap();
            let stack = ViewStack::new(view);

            assert_eq!(
                stack.screen_rect(),
                Some(tc.expected_screen),
                "screen_rect failed for '{}'",
                tc.name
            );
            assert_eq!(
                stack.canvas_rect(),
                Some(tc.expected_canvas),
                "canvas_rect failed for '{}'",
                tc.name
            );
        }
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
                viewport2: ((10, 10), (0, 0, 10, 10), (2, 2)),
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
                name: "Second viewport positioned outside first",
                viewport1: ((10, 10), (0, 0, 10, 10), (0, 0)),
                viewport2: ((10, 10), (0, 0, 10, 10), (11, 11)),
                expected_screen: None,
                expected_canvas: None,
            },
            TestCase {
                name: "Partial overlap",
                viewport1: ((100, 100), (0, 0, 50, 50), (0, 0)),
                viewport2: ((100, 100), (0, 0, 50, 50), (25, 25)),
                expected_screen: Some(Rect::new(25, 25, 25, 25)),
                expected_canvas: Some(Rect::new(0, 0, 25, 25)),
            },
            TestCase {
                name: "Complex view positioning",
                viewport1: ((100, 100), (0, 0, 80, 80), (10, 10)),
                viewport2: ((80, 80), (10, 10, 40, 40), (20, 20)),
                expected_screen: Some(Rect::new(40, 40, 40, 40)),
                expected_canvas: Some(Rect::new(10, 10, 40, 40)),
            },
        ];

        for tc in test_cases {
            let view1 = ViewPort::new(tc.viewport1.0, tc.viewport1.1, tc.viewport1.2).unwrap();
            let mut stack = ViewStack::new(view1);

            let view2 = ViewPort::new(tc.viewport2.0, tc.viewport2.1, tc.viewport2.2).unwrap();
            stack.push(view2);

            assert_eq!(
                stack.screen_rect(),
                tc.expected_screen,
                "screen_rect failed for '{}'",
                tc.name
            );
            assert_eq!(
                stack.canvas_rect(),
                tc.expected_canvas,
                "canvas_rect failed for '{}'",
                tc.name
            );
        }
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
        assert_eq!(stack.screen_rect(), Some(Rect::new(30, 30, 60, 60)));
        assert_eq!(stack.canvas_rect(), Some(Rect::new(0, 0, 60, 60)));
    }
}
