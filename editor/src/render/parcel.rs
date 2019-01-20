use crate::objects::{Ctx, ID};
use crate::render::{RenderOptions, Renderable, PARCEL_BOUNDARY_THICKNESS};
use ezgui::{Color, GfxCtx};
use geom::{Bounds, PolyLine, Polygon, Pt2D};
use map_model::{Parcel, ParcelID};

const COLORS: [Color; 14] = [
    // TODO these are awful choices
    Color::rgba_f(1.0, 1.0, 0.0, 1.0),
    Color::rgba_f(1.0, 0.0, 1.0, 1.0),
    Color::rgba_f(0.0, 1.0, 1.0, 1.0),
    Color::rgba_f(0.5, 0.2, 0.7, 1.0),
    Color::rgba_f(0.5, 0.5, 0.0, 0.5),
    Color::rgba_f(0.5, 0.0, 0.5, 0.5),
    Color::rgba_f(0.0, 0.5, 0.5, 0.5),
    Color::rgba_f(0.0, 0.0, 0.5, 0.5),
    Color::rgba_f(0.3, 0.2, 0.5, 0.5),
    Color::rgba_f(0.4, 0.2, 0.5, 0.5),
    Color::rgba_f(0.5, 0.2, 0.5, 0.5),
    Color::rgba_f(0.6, 0.2, 0.5, 0.5),
    Color::rgba_f(0.7, 0.2, 0.5, 0.5),
    Color::rgba_f(0.8, 0.2, 0.5, 0.5),
];

#[derive(Debug)]
pub struct DrawParcel {
    pub id: ParcelID,
    // TODO bit wasteful to keep both
    boundary_polygon: Polygon,
    pub fill_polygon: Polygon,
}

impl DrawParcel {
    pub fn new(p: &Parcel) -> DrawParcel {
        DrawParcel {
            id: p.id,
            boundary_polygon: PolyLine::new(p.points.clone())
                .make_polygons(PARCEL_BOUNDARY_THICKNESS),
            fill_polygon: Polygon::new(&p.points),
        }
    }
}

impl Renderable for DrawParcel {
    fn get_id(&self) -> ID {
        ID::Parcel(self.id)
    }

    fn draw(&self, g: &mut GfxCtx, opts: RenderOptions, ctx: &Ctx) {
        let color = opts.color.unwrap_or_else(|| {
            let p = ctx.map.get_p(self.id);
            COLORS[p.block % COLORS.len()]
        });
        g.draw_polygon(color, &self.fill_polygon);

        g.draw_polygon(
            ctx.cs.get_def("parcel boundary", Color::grey(0.3)),
            &self.boundary_polygon,
        );
    }

    fn get_bounds(&self) -> Bounds {
        self.fill_polygon.get_bounds()
    }

    fn contains_pt(&self, pt: Pt2D) -> bool {
        self.fill_polygon.contains_pt(pt)
    }
}
