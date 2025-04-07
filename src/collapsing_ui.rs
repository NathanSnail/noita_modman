use egui::{
    collapsing_header::{paint_default_icon, CollapsingState},
    epaint, pos2, CollapsingResponse, Id, InnerResponse, Rect, Response, Sense, StrokeKind, Ui,
    WidgetInfo, WidgetType,
};

struct Prepared<T> {
    header_response: Response,
    state: CollapsingState,
    openness: f32,
    inner: T,
}

/// inner represents the header
pub struct InnerCollapsingResponse<T, R> {
    pub inner: T,
    pub response: CollapsingResponse<R>,
}

/// Based off [`egui::containers::collapsing_header`]
pub struct CollapsingUi<T> {
    render: Box<dyn FnMut(&mut Ui) -> InnerResponse<(T, Rect)>>,
    default_open: bool,
    open: Option<bool>,
    id_salt: Id,
    selectable: bool,
    selected: bool,
    show_background: bool,
}

impl<T> CollapsingUi<T> {
    pub fn new(
        id_salt: Id,
        render_fn: Box<dyn FnMut(&mut Ui) -> InnerResponse<(T, Rect)>>,
    ) -> Self {
        Self {
            render: render_fn,
            default_open: false,
            open: None,
            id_salt,
            selectable: false,
            selected: false,
            show_background: false,
        }
    }

    fn begin(self, ui: &mut Ui) -> Prepared<T> {
        assert!(
            ui.layout().main_dir().is_vertical(),
            "Horizontal collapsing is unimplemented"
        );
        let Self {
            mut render,
            default_open,
            open,
            id_salt,
            selectable,
            selected,
            show_background,
        } = self;

        let id = ui.make_persistent_id(id_salt);
        let button_padding = ui.spacing().button_padding;

        ui.horizontal(|ui| {
            let icon_minimal = ui.allocate_space(button_padding);

            let (mut icon_rect, _) = ui.spacing().icon_rectangles(icon_minimal.1);

            icon_rect.set_center(pos2(
                icon_rect.left() + ui.spacing().indent / 2.0,
                icon_rect.center().y,
            ));

            let icon_response = ui.allocate_rect(icon_rect, Sense::click());
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
            let openness = state.openness(ui.ctx());

            paint_default_icon(ui, openness, &icon_response);
            let inner_response = render(ui);
            let rect = inner_response.inner.1;

            let mut header_response = ui.interact(rect, id, Sense::click());

            if let Some(open) = open {
                if open != state.is_open() {
                    state.toggle(ui);
                    header_response.mark_changed();
                }
            } else if header_response.clicked() {
                state.toggle(ui);
                header_response.mark_changed();
            }

            header_response.widget_info(|| {
                WidgetInfo::labeled(WidgetType::CollapsingHeader, ui.is_enabled(), "")
            });

            let visuals = ui.style().interact_selectable(&header_response, selected);

            if ui.visuals().collapsing_header_frame || show_background {
                ui.painter().add(epaint::RectShape::new(
                    header_response.rect.expand(visuals.expansion),
                    visuals.corner_radius,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                    StrokeKind::Inside,
                ));
            }

            if selected || selectable && (header_response.hovered() || header_response.has_focus())
            {
                let rect = rect.expand(visuals.expansion);

                ui.painter().rect(
                    rect,
                    visuals.corner_radius,
                    visuals.bg_fill,
                    visuals.bg_stroke,
                    StrokeKind::Inside,
                );
            }
            Prepared {
                header_response,
                state,
                openness,
                inner: inner_response.inner.0,
            }
        })
        .inner
    }

    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_body: impl FnOnce(&mut Ui) -> R,
    ) -> InnerCollapsingResponse<T, R> {
        self.show_dyn(ui, Box::new(add_body), true)
    }

    fn show_dyn<'c, R>(
        self,
        ui: &mut Ui,
        add_body: Box<dyn FnOnce(&mut Ui) -> R + 'c>,
        indented: bool,
    ) -> InnerCollapsingResponse<T, R> {
        // Make sure body is bellow header,
        // and make sure it is one unit (necessary for putting a [`CollapsingHeader`] in a grid).
        ui.vertical(|ui| {
            let Prepared {
                header_response,
                mut state,
                openness,
                inner,
            } = self.begin(ui); // show the header

            let ret_response = if indented {
                state.show_body_indented(&header_response, ui, add_body)
            } else {
                state.show_body_unindented(ui, add_body)
            };

            InnerCollapsingResponse {
                inner,
                response: if let Some(ret_response) = ret_response {
                    CollapsingResponse {
                        header_response,
                        body_response: Some(ret_response.response),
                        body_returned: Some(ret_response.inner),
                        openness,
                    }
                } else {
                    CollapsingResponse {
                        header_response,
                        body_response: None,
                        body_returned: None,
                        openness,
                    }
                },
            }
        })
        .inner
    }
}
