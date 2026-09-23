use crate::{ui_icon_shapes::IconFrame, ui_icons::IconKind};
use egui::{Color32, Stroke, StrokeKind, Ui};

pub(super) fn draw_tool_icon(ui: &Ui, frame: &IconFrame, icon: IconKind, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.6_f32, color);
    let thin = Stroke::new(1.25_f32, color);

    match icon {
        IconKind::Command => {
            for (x, y) in [(8.0, 8.0), (16.0, 8.0), (8.0, 16.0), (16.0, 16.0)] {
                painter.circle_stroke(frame.p(x, y), frame.rect().width() * 0.12, thin);
            }
            painter.line_segment([frame.p(8.0, 11.0), frame.p(8.0, 13.0)], stroke);
            painter.line_segment([frame.p(16.0, 11.0), frame.p(16.0, 13.0)], stroke);
            painter.line_segment([frame.p(11.0, 8.0), frame.p(13.0, 8.0)], stroke);
            painter.line_segment([frame.p(11.0, 16.0), frame.p(13.0, 16.0)], stroke);
            painter.rect_stroke(
                frame.rr(8.0, 8.0, 16.0, 16.0),
                2.0,
                thin,
                StrokeKind::Inside,
            );
        }
        IconKind::Search => {
            painter.circle_stroke(frame.p(10.5, 10.5), frame.rect().width() * 0.24, stroke);
            painter.line_segment([frame.p(15.0, 15.0), frame.p(20.0, 20.0)], stroke);
        }
        IconKind::Terminal => {
            painter.rect_stroke(
                frame.rr(4.0, 5.0, 20.0, 19.0),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment([frame.p(7.0, 10.0), frame.p(10.0, 12.0)], stroke);
            painter.line_segment([frame.p(10.0, 12.0), frame.p(7.0, 14.0)], stroke);
            painter.line_segment([frame.p(12.0, 15.0), frame.p(17.0, 15.0)], stroke);
        }
        IconKind::Trash => {
            painter.line_segment([frame.p(8.0, 7.0), frame.p(16.0, 7.0)], stroke);
            painter.line_segment([frame.p(10.0, 5.0), frame.p(14.0, 5.0)], stroke);
            painter.rect_stroke(
                frame.rr(7.0, 8.5, 17.0, 20.0),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment([frame.p(10.0, 11.0), frame.p(10.0, 17.0)], thin);
            painter.line_segment([frame.p(14.0, 11.0), frame.p(14.0, 17.0)], thin);
        }
        IconKind::Copy => {
            painter.rect_stroke(
                frame.rr(8.0, 5.0, 18.0, 15.0),
                1.5,
                thin,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                frame.rr(6.0, 9.0, 16.0, 19.0),
                1.5,
                stroke,
                StrokeKind::Inside,
            );
        }
        IconKind::GitBranch => {
            painter.line_segment([frame.p(8.0, 7.5), frame.p(8.0, 16.5)], stroke);
            painter.line_segment([frame.p(8.0, 12.0), frame.p(16.0, 8.0)], stroke);
            painter.circle_stroke(frame.p(8.0, 6.5), frame.rect().width() * 0.11, stroke);
            painter.circle_stroke(frame.p(8.0, 17.5), frame.rect().width() * 0.11, stroke);
            painter.circle_stroke(frame.p(17.0, 7.5), frame.rect().width() * 0.11, stroke);
        }
        IconKind::Diagnostics => {
            painter.line_segment([frame.p(12.0, 4.5), frame.p(21.0, 19.0)], stroke);
            painter.line_segment([frame.p(21.0, 19.0), frame.p(3.0, 19.0)], stroke);
            painter.line_segment([frame.p(3.0, 19.0), frame.p(12.0, 4.5)], stroke);
            painter.line_segment([frame.p(12.0, 9.0), frame.p(12.0, 14.0)], stroke);
            painter.circle_filled(frame.p(12.0, 16.7), frame.rect().width() * 0.045, color);
        }
        IconKind::Lsp => {
            painter.line_segment([frame.p(7.0, 12.0), frame.p(12.0, 7.0)], thin);
            painter.line_segment([frame.p(12.0, 7.0), frame.p(17.0, 12.0)], thin);
            painter.line_segment([frame.p(7.0, 12.0), frame.p(12.0, 17.0)], thin);
            painter.line_segment([frame.p(12.0, 17.0), frame.p(17.0, 12.0)], thin);
            for (x, y) in [(7.0, 12.0), (12.0, 7.0), (17.0, 12.0), (12.0, 17.0)] {
                painter.circle_filled(frame.p(x, y), frame.rect().width() * 0.09, color);
            }
        }
        IconKind::Cursor => {
            painter.line_segment([frame.p(12.0, 5.0), frame.p(12.0, 19.0)], stroke);
            painter.line_segment([frame.p(8.0, 8.0), frame.p(16.0, 8.0)], thin);
            painter.line_segment([frame.p(8.0, 16.0), frame.p(16.0, 16.0)], thin);
        }
        IconKind::Keyboard => {
            painter.rect_stroke(
                frame.rr(4.0, 6.5, 20.0, 17.5),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            for (x1, x2) in [
                (6.8, 8.2),
                (9.4, 10.8),
                (12.0, 13.4),
                (14.6, 16.0),
                (17.2, 18.6),
            ] {
                painter.line_segment([frame.p(x1, 10.0), frame.p(x2, 10.0)], thin);
            }
            for (x1, x2) in [
                (6.8, 8.2),
                (9.4, 10.8),
                (12.0, 13.4),
                (14.6, 16.0),
                (17.2, 18.6),
            ] {
                painter.line_segment([frame.p(x1, 13.2), frame.p(x2, 13.2)], thin);
            }
            painter.line_segment([frame.p(8.0, 15.4), frame.p(16.0, 15.4)], thin);
        }
        IconKind::Theme => {
            painter.circle_stroke(frame.p(12.0, 12.0), frame.rect().width() * 0.18, stroke);
            for (x1, y1, x2, y2) in [
                (12.0, 3.5, 12.0, 6.0),
                (12.0, 18.0, 12.0, 20.5),
                (3.5, 12.0, 6.0, 12.0),
                (18.0, 12.0, 20.5, 12.0),
                (6.0, 6.0, 7.8, 7.8),
                (16.2, 16.2, 18.0, 18.0),
                (18.0, 6.0, 16.2, 7.8),
                (7.8, 16.2, 6.0, 18.0),
            ] {
                painter.line_segment([frame.p(x1, y1), frame.p(x2, y2)], thin);
            }
        }
        IconKind::Code => {
            painter.line_segment([frame.p(10.0, 7.0), frame.p(6.0, 12.0)], stroke);
            painter.line_segment([frame.p(6.0, 12.0), frame.p(10.0, 17.0)], stroke);
            painter.line_segment([frame.p(14.0, 7.0), frame.p(18.0, 12.0)], stroke);
            painter.line_segment([frame.p(18.0, 12.0), frame.p(14.0, 17.0)], stroke);
        }
        IconKind::Settings => {
            let center = frame.p(12.0, 12.0);
            let outer = frame.rect().width() * 0.34;
            let inner = frame.rect().width() * 0.13;
            painter.circle_stroke(center, outer * 0.58, stroke);
            painter.circle_stroke(center, inner, thin);
            for (x1, y1, x2, y2) in [
                (12.0, 3.8, 12.0, 6.0),
                (12.0, 18.0, 12.0, 20.2),
                (3.8, 12.0, 6.0, 12.0),
                (18.0, 12.0, 20.2, 12.0),
                (6.2, 6.2, 7.8, 7.8),
                (16.2, 16.2, 17.8, 17.8),
                (17.8, 6.2, 16.2, 7.8),
                (7.8, 16.2, 6.2, 17.8),
            ] {
                painter.line_segment([frame.p(x1, y1), frame.p(x2, y2)], thin);
            }
        }
        _ => {}
    }
}
