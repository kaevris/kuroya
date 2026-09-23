use crate::{ui_icon_primitives::arc_points, ui_icon_shapes::IconFrame, ui_icons::IconKind};
use egui::{Color32, Shape, Stroke, StrokeKind, Ui};

pub(super) fn draw_chrome_icon(ui: &Ui, frame: &IconFrame, icon: IconKind, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.6_f32, color);
    let thin = Stroke::new(1.25_f32, color);

    match icon {
        IconKind::ChevronRight => {
            painter.line_segment([frame.p(9.0, 6.0), frame.p(15.0, 12.0)], stroke);
            painter.line_segment([frame.p(15.0, 12.0), frame.p(9.0, 18.0)], stroke);
        }
        IconKind::ChevronDown => {
            painter.line_segment([frame.p(6.0, 9.0), frame.p(12.0, 15.0)], stroke);
            painter.line_segment([frame.p(12.0, 15.0), frame.p(18.0, 9.0)], stroke);
        }
        IconKind::Plus => {
            painter.line_segment([frame.p(6.0, 12.0), frame.p(18.0, 12.0)], stroke);
            painter.line_segment([frame.p(12.0, 6.0), frame.p(12.0, 18.0)], stroke);
        }
        IconKind::Minus => {
            painter.line_segment([frame.p(6.0, 12.0), frame.p(18.0, 12.0)], stroke);
        }
        IconKind::Refresh => {
            let center = frame.p(12.0, 12.0);
            let radius = frame.rect().width() * 0.29;
            painter.add(Shape::line(arc_points(center, radius, -0.15, 4.55), stroke));
            painter.line_segment([frame.p(17.0, 4.5), frame.p(20.0, 5.5)], stroke);
            painter.line_segment([frame.p(17.0, 4.5), frame.p(17.5, 7.8)], stroke);
        }
        IconKind::Maximize => {
            painter.rect_stroke(
                frame.rr(6.0, 6.0, 18.0, 18.0),
                1.5,
                stroke,
                StrokeKind::Inside,
            );
        }
        IconKind::Restore => {
            painter.rect_stroke(
                frame.rr(8.0, 6.0, 18.0, 16.0),
                1.5,
                thin,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                frame.rr(6.0, 8.0, 16.0, 18.0),
                1.5,
                stroke,
                StrokeKind::Inside,
            );
        }
        IconKind::Close => {
            painter.line_segment([frame.p(7.0, 7.0), frame.p(17.0, 17.0)], stroke);
            painter.line_segment([frame.p(17.0, 7.0), frame.p(7.0, 17.0)], stroke);
        }
        IconKind::Panes => {
            painter.rect_stroke(
                frame.rr(4.0, 5.0, 20.0, 19.0),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment([frame.p(12.0, 5.0), frame.p(12.0, 19.0)], thin);
            painter.line_segment([frame.p(4.0, 10.0), frame.p(20.0, 10.0)], thin);
        }
        _ => {}
    }
}
