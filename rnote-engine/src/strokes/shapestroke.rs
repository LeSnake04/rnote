use super::StrokeBehaviour;
use crate::{render, DrawBehaviour};
use rnote_compose::shapes::Shape;
use rnote_compose::shapes::ShapeBehaviour;
use rnote_compose::style::Composer;
use rnote_compose::transform::TransformBehaviour;
use rnote_compose::Style;

use p2d::bounding_volume::AABB;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "shapestroke")]
pub struct ShapeStroke {
    #[serde(rename = "shape")]
    pub shape: Shape,
    #[serde(rename = "style")]
    pub style: Style,
}

impl StrokeBehaviour for ShapeStroke {
    fn gen_svg(&self) -> Result<crate::render::Svg, anyhow::Error> {
        let bounds = self.bounds();
        let mut cx = piet_svg::RenderContext::new_no_text(kurbo::Size::new(
            bounds.extents()[0],
            bounds.extents()[1],
        ));

        self.draw(&mut cx, 1.0)?;
        let svg_data = rnote_compose::utils::piet_svg_cx_to_svg(cx)?;

        Ok(render::Svg { svg_data, bounds })
    }

    fn gen_images(&self, image_scale: f64) -> Result<Vec<render::Image>, anyhow::Error> {
        Ok(render::Image::gen_images_from_drawable(
            self,
            self.bounds(),
            image_scale,
        )?)
    }
}

impl DrawBehaviour for ShapeStroke {
    fn draw(&self, cx: &mut impl piet::RenderContext, _image_scale: f64) -> anyhow::Result<()> {
        match self.shape {
            Shape::Line(ref line) => match &self.style {
                Style::Smooth(options) => {
                    line.draw_composed(cx, options);
                }
                Style::Rough(options) => {
                    line.draw_composed(cx, options);
                }
                Style::Textured(_) => {}
            },
            Shape::Rectangle(ref rectangle) => match &self.style {
                Style::Smooth(options) => {
                    rectangle.draw_composed(cx, options);
                }
                Style::Rough(options) => {
                    rectangle.draw_composed(cx, options);
                }
                Style::Textured(_) => {}
            },
            Shape::Ellipse(ref ellipse) => match &self.style {
                Style::Smooth(options) => {
                    ellipse.draw_composed(cx, options);
                }
                Style::Rough(options) => {
                    ellipse.draw_composed(cx, options);
                }
                Style::Textured(_) => {}
            },
        };

        Ok(())
    }
}

impl ShapeBehaviour for ShapeStroke {
    fn bounds(&self) -> AABB {
        match &self.style {
            Style::Smooth(options) => self.shape.composed_bounds(options),
            Style::Rough(options) => self.shape.composed_bounds(options),
            Style::Textured(_) => self.shape.bounds(),
        }
    }
}

impl TransformBehaviour for ShapeStroke {
    fn translate(&mut self, offset: nalgebra::Vector2<f64>) {
        self.shape.translate(offset);
    }
    fn rotate(&mut self, angle: f64, center: nalgebra::Point2<f64>) {
        self.shape.rotate(angle, center);
    }
    fn scale(&mut self, scale: nalgebra::Vector2<f64>) {
        self.shape.scale(scale);
    }
}

impl ShapeStroke {
    pub fn new(shape: Shape, style: Style) -> Self {
        Self { shape, style }
    }
}
