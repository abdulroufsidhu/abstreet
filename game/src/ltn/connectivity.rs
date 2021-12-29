use std::collections::HashSet;

use geom::Distance;
use widgetry::mapspace::World;
use widgetry::{
    EventCtx, GeomBatch, GfxCtx, Key, Outcome, Panel, State, Text, TextExt, Toggle, Widget,
};

use super::per_neighborhood::{FilterableObj, Tab, TakeNeighborhood};
use super::Neighborhood;
use crate::app::{App, Transition};

pub struct Viewer {
    panel: Panel,
    neighborhood: Neighborhood,
    world: World<FilterableObj>,
}

impl TakeNeighborhood for Viewer {
    fn take_neighborhood(self) -> Neighborhood {
        self.neighborhood
    }
}

impl Viewer {
    pub fn new_state(
        ctx: &mut EventCtx,
        app: &App,
        neighborhood: Neighborhood,
    ) -> Box<dyn State<App>> {
        let panel = Tab::Connectivity
            .panel_builder(
                ctx,
                app,
                Widget::col(vec![
                    Widget::row(vec![
                        "Draw traffic cells as".text_widget(ctx).centered_vert(),
                        Toggle::choice(ctx, "draw cells", "areas", "streets", Key::D, true),
                    ]),
                    Text::new().into_widget(ctx).named("warnings"),
                ]),
            )
            .build(ctx);

        let mut viewer = Viewer {
            panel,
            neighborhood,
            world: World::unbounded(),
        };
        viewer.neighborhood_changed(ctx, app);
        Box::new(viewer)
    }

    fn neighborhood_changed(&mut self, ctx: &mut EventCtx, app: &App) {
        self.world = make_world(
            ctx,
            app,
            &self.neighborhood,
            self.panel.is_checked("draw cells"),
        );
        let disconnected_cells = self
            .neighborhood
            .cells
            .iter()
            .filter(|c| c.borders.is_empty())
            .count();
        // TODO Also add a red outline to them or something
        let warning = if disconnected_cells == 0 {
            String::new()
        } else {
            format!("{} cells are totally disconnected", disconnected_cells)
        };
        self.panel
            .replace(ctx, "warnings", warning.text_widget(ctx));
    }
}

impl State<App> for Viewer {
    fn event(&mut self, ctx: &mut EventCtx, app: &mut App) -> Transition {
        match self.panel.event(ctx) {
            Outcome::Clicked(x) => {
                return Tab::Connectivity.must_handle_action::<Viewer>(ctx, app, x.as_ref());
            }
            Outcome::Changed(_) => {
                self.world = make_world(
                    ctx,
                    app,
                    &self.neighborhood,
                    self.panel.is_checked("draw cells"),
                );
            }
            _ => {}
        }

        let world_outcome = self.world.event(ctx);
        if super::per_neighborhood::handle_world_outcome(ctx, app, world_outcome) {
            // TODO The cell coloring changes quite spuriously just by toggling a filter, even when
            // it doesn't matter
            self.neighborhood =
                Neighborhood::new(ctx, app, self.neighborhood.orig_perimeter.clone());
            self.neighborhood_changed(ctx, app);
        }

        Transition::Keep
    }

    fn draw(&self, g: &mut GfxCtx, app: &App) {
        self.panel.draw(g);
        g.redraw(&self.neighborhood.fade_irrelevant);
        self.world.draw(g);
        self.neighborhood.draw_filters.draw(g);
        // TODO Since we cover such a small area, treating multiple segments of one road as the
        // same might be nice. And we should seed the quadtree with the locations of filters and
        // arrows, possibly.
        if g.canvas.is_unzoomed() {
            self.neighborhood.labels.draw(g, app);
        }
    }
}

fn make_world(
    ctx: &mut EventCtx,
    app: &App,
    neighborhood: &Neighborhood,
    draw_cells_as_areas: bool,
) -> World<FilterableObj> {
    let map = &app.primary.map;
    let mut world = World::bounded(map.get_bounds());

    super::per_neighborhood::populate_world(ctx, app, neighborhood, &mut world, |id| id, 0);

    if draw_cells_as_areas {
        world.draw_master_batch(ctx, super::draw_cells::draw_cells(map, neighborhood));
    } else {
        let mut draw = GeomBatch::new();
        let mut debug_cell_borders = GeomBatch::new();
        let mut seen_roads = HashSet::new();
        for (idx, cell) in neighborhood.cells.iter().enumerate() {
            let color = super::draw_cells::COLORS[idx % super::draw_cells::COLORS.len()].alpha(0.9);
            for r in cell.roads.keys() {
                // TODO Roads with a filter belong to two cells. The drawn form (and the
                // intersections included) needs to be adjusted to use two colors.
                if seen_roads.contains(r) {
                    continue;
                }
                seen_roads.insert(*r);

                draw.push(color, map.get_r(*r).get_thick_polygon());
            }
            for i in
                crate::common::intersections_from_roads(&cell.roads.keys().cloned().collect(), map)
            {
                draw.push(color, map.get_i(i).polygon.clone());
            }
            // Draw the cell borders as outlines, for debugging. (Later, we probably want some kind
            // of arrow styling)
            for i in &cell.borders {
                if let Ok(p) = map.get_i(*i).polygon.to_outline(Distance::meters(2.0)) {
                    debug_cell_borders.push(color.alpha(1.0), p);
                }
            }
        }
        draw.append(debug_cell_borders);
        world.draw_master_batch(ctx, draw);
    }

    world.initialize_hover(ctx);

    world
}