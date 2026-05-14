use crate::config::RenderMode;
use crate::layout::{DockLayout, Point};
use crate::model::DockModel;
use crate::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub struct ShelfRenderResult {
    pub rendered: bool,
    pub fallback_reason: Option<String>,
}

impl ShelfRenderResult {
    pub fn rendered() -> Self {
        Self {
            rendered: true,
            fallback_reason: None,
        }
    }

    pub fn fallback(reason: impl Into<String>) -> Self {
        Self {
            rendered: false,
            fallback_reason: Some(reason.into()),
        }
    }
}

pub trait ShelfRenderer {
    fn kind(&self) -> RenderMode;
    fn resize(&mut self, size: (i32, i32), scale_factor: f64);
    fn render_shelf(
        &mut self,
        layout: &DockLayout,
        model: &DockModel,
        theme: &Theme,
        hover: Option<Point>,
    ) -> ShelfRenderResult;
    fn fallback_reason(&self) -> Option<&str>;
}

#[derive(Debug, Default)]
pub struct Procedural2dRenderer {
    size: (i32, i32),
    scale_factor: f64,
}

#[derive(Debug, Default)]
pub struct Texture2dRenderer {
    size: (i32, i32),
    scale_factor: f64,
    fallback_reason: Option<String>,
}

impl ShelfRenderer for Procedural2dRenderer {
    fn kind(&self) -> RenderMode {
        RenderMode::Procedural2d
    }

    fn resize(&mut self, size: (i32, i32), scale_factor: f64) {
        self.size = size;
        self.scale_factor = scale_factor;
    }

    fn render_shelf(
        &mut self,
        _layout: &DockLayout,
        _model: &DockModel,
        _theme: &Theme,
        _hover: Option<Point>,
    ) -> ShelfRenderResult {
        ShelfRenderResult::rendered()
    }

    fn fallback_reason(&self) -> Option<&str> {
        None
    }
}

impl ShelfRenderer for Texture2dRenderer {
    fn kind(&self) -> RenderMode {
        RenderMode::Texture2d
    }

    fn resize(&mut self, size: (i32, i32), scale_factor: f64) {
        self.size = size;
        self.scale_factor = scale_factor;
    }

    fn render_shelf(
        &mut self,
        _layout: &DockLayout,
        _model: &DockModel,
        theme: &Theme,
        _hover: Option<Point>,
    ) -> ShelfRenderResult {
        if theme.assets.fallback_texture.is_some() {
            self.fallback_reason = None;
            ShelfRenderResult::rendered()
        } else {
            let reason = "texture-2d theme did not provide a fallback shelf texture".to_string();
            self.fallback_reason = Some(reason.clone());
            ShelfRenderResult::fallback(reason)
        }
    }

    fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeConfig;

    #[test]
    fn texture_renderer_requires_texture_asset() {
        let mut renderer = Texture2dRenderer::default();
        let theme = Theme::from_config(&ThemeConfig::default());
        let result =
            renderer.render_shelf(&DockLayout::default(), &DockModel::default(), &theme, None);

        assert!(!result.rendered);
        assert!(renderer.fallback_reason().is_some());
    }
}
