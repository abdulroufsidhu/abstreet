use maplit::btreeset;

use crate::ID;
use abstutil::{prettyprint_usize, Counter};
use geom::{Distance, Time};
use map_gui::tools::{ColorDiscrete, ColorNetwork};
use map_model::{AmenityType, Direction, LaneType};
use sim::AgentType;
use widgetry::mapspace::ToggleZoomed;
use widgetry::tools::ColorLegend;
use widgetry::{Color, EventCtx, GfxCtx, Line, Panel, Text, Widget};

use crate::app::App;
use crate::layer::{header, Layer, LayerOutcome, PANEL_PLACEMENT};

pub struct BikeActivity {
    panel: Panel,
    time: Time,
    draw: ToggleZoomed,
    tooltip: Option<Text>,
}

impl Layer for BikeActivity {
    fn name(&self) -> Option<&'static str> {
        Some("cycling activity")
    }
    fn event(&mut self, ctx: &mut EventCtx, app: &mut App) -> Option<LayerOutcome> {
        let mut recalc_tooltip = false;
        if app.primary.sim.time() != self.time {
            *self = BikeActivity::new(ctx, app);
            recalc_tooltip = true;
        }

        // Show a tooltip with count, only when unzoomed
        if ctx.canvas.is_unzoomed() {
            if ctx.redo_mouseover() || recalc_tooltip {
                self.tooltip = None;
                if let Some(ID::Road(r)) = app.mouseover_unzoomed_roads_and_intersections(ctx) {
                    let cnt = app
                        .primary
                        .sim
                        .get_analytics()
                        .road_thruput
                        .total_for_with_agent_types(r, btreeset! { AgentType::Bike });
                    if cnt > 0 {
                        self.tooltip = Some(Text::from(prettyprint_usize(cnt)));
                    }
                }
            }
        } else {
            self.tooltip = None;
        }

        <dyn Layer>::simple_event(ctx, &mut self.panel)
    }
    fn draw(&self, g: &mut GfxCtx, _: &App) {
        self.panel.draw(g);
        self.draw.draw(g);
        if let Some(ref txt) = self.tooltip {
            g.draw_mouse_tooltip(txt.clone());
        }
    }
    fn draw_minimap(&self, g: &mut GfxCtx) {
        g.redraw(&self.draw.unzoomed);
    }
}

impl BikeActivity {
    pub fn new(ctx: &mut EventCtx, app: &App) -> BikeActivity {
        let mut num_lanes = 0;
        let mut total_dist = Distance::ZERO;
        let mut on_bike_lanes = Counter::new();
        let mut off_bike_lanes = Counter::new();
        let mut intersections_on = Counter::new();
        let mut intersections_off = Counter::new();
        // Make sure all bikes lanes show up no matter what
        for l in app.primary.map.all_lanes() {
            if l.is_biking() {
                on_bike_lanes.add(l.id.road, 0);
                intersections_on.add(l.src_i, 0);
                intersections_on.add(l.src_i, 0);
                num_lanes += 1;
                total_dist += l.length();
            }
        }

        // Show throughput, broken down by bike lanes or not
        for ((r, agent_type, _), count) in &app.primary.sim.get_analytics().road_thruput.counts {
            if *agent_type == AgentType::Bike {
                if app
                    .primary
                    .map
                    .get_r(*r)
                    .lanes
                    .iter()
                    .any(|l| l.lane_type == LaneType::Biking)
                {
                    on_bike_lanes.add(*r, *count);
                } else {
                    off_bike_lanes.add(*r, *count);
                }
            }
        }

        // Use intersection data too, but bin as on bike lanes or not based on connecting roads
        for ((i, agent_type, _), count) in
            &app.primary.sim.get_analytics().intersection_thruput.counts
        {
            if *agent_type == AgentType::Bike {
                if app
                    .primary
                    .map
                    .get_i(*i)
                    .roads
                    .iter()
                    .any(|r| on_bike_lanes.get(*r) > 0)
                {
                    intersections_on.add(*i, *count);
                } else {
                    intersections_off.add(*i, *count);
                }
            }
        }

        let panel = Panel::new_builder(Widget::col(vec![
            header(ctx, "Cycling activity"),
            Text::from_multiline(vec![
                Line(format!("{} bike lanes", num_lanes)),
                Line(format!(
                    "total distance of {}",
                    total_dist.to_string(&app.opts.units)
                )),
            ])
            .into_widget(ctx),
            Line("Throughput on bike lanes").into_widget(ctx),
            ColorLegend::gradient(
                ctx,
                &app.cs.good_to_bad_green,
                vec!["lowest count", "highest"],
            ),
            Line("Throughput on unprotected roads").into_widget(ctx),
            ColorLegend::gradient(
                ctx,
                &app.cs.good_to_bad_red,
                vec!["lowest count", "highest"],
            ),
        ]))
        .aligned_pair(PANEL_PLACEMENT)
        .build(ctx);

        let mut colorer = ColorNetwork::new(app);
        colorer.ranked_roads(on_bike_lanes, &app.cs.good_to_bad_green);
        colorer.ranked_roads(off_bike_lanes, &app.cs.good_to_bad_red);
        colorer.ranked_intersections(intersections_on, &app.cs.good_to_bad_green);
        colorer.ranked_intersections(intersections_off, &app.cs.good_to_bad_red);

        BikeActivity {
            panel,
            time: app.primary.sim.time(),
            draw: colorer.build(ctx),
            tooltip: None,
        }
    }
}

pub struct Static {
    panel: Panel,
    pub draw: ToggleZoomed,
    name: &'static str,
}

impl Layer for Static {
    fn name(&self) -> Option<&'static str> {
        Some(self.name)
    }
    fn event(&mut self, ctx: &mut EventCtx, _: &mut App) -> Option<LayerOutcome> {
        <dyn Layer>::simple_event(ctx, &mut self.panel)
    }
    fn draw(&self, g: &mut GfxCtx, _: &App) {
        self.panel.draw(g);
        self.draw.draw(g);
    }
    fn draw_minimap(&self, g: &mut GfxCtx) {
        g.redraw(&self.draw.unzoomed);
    }
}

impl Static {
    fn new(
        ctx: &mut EventCtx,
        colorer: ColorDiscrete,
        name: &'static str,
        title: String,
        extra: Widget,
    ) -> Static {
        let (draw, legend) = colorer.build(ctx);
        let panel = Panel::new_builder(Widget::col(vec![header(ctx, &title), extra, legend]))
            .aligned_pair(PANEL_PLACEMENT)
            .build(ctx);

        Static { panel, draw, name }
    }

    pub fn edits(ctx: &mut EventCtx, app: &App) -> Static {
        let mut colorer = ColorDiscrete::new(
            app,
            vec![("modified road/intersection", app.cs.edits_layer)],
        );

        let edits = app.primary.map.get_edits();
        let (lanes, roads) = edits.changed_lanes(&app.primary.map);
        for l in lanes {
            colorer.add_l(l, "modified road/intersection");
        }
        for r in roads {
            colorer.add_r(r, "modified road/intersection");
        }
        for i in edits.original_intersections.keys() {
            colorer.add_i(*i, "modified road/intersection");
        }

        Static::new(
            ctx,
            colorer,
            "map edits",
            format!("Map edits ({})", edits.edits_name),
            Text::from_multiline(vec![
                Line(format!("{} roads changed", edits.changed_roads.len())),
                Line(format!(
                    "{} intersections changed",
                    edits.original_intersections.len()
                )),
            ])
            .into_widget(ctx),
        )
    }

    pub fn amenities(ctx: &mut EventCtx, app: &App) -> Static {
        let food = Color::RED;
        let school = Color::CYAN;
        let shopping = Color::PURPLE;
        let other = Color::GREEN;

        let mut draw = ToggleZoomed::builder();
        for b in app.primary.map.all_buildings() {
            if b.amenities.is_empty() {
                continue;
            }
            let mut color = None;
            for a in &b.amenities {
                if let Some(t) = AmenityType::categorize(&a.amenity_type) {
                    color = Some(match t {
                        AmenityType::Food => food,
                        AmenityType::School => school,
                        AmenityType::Shopping => shopping,
                        _ => other,
                    });
                    break;
                }
            }
            let color = color.unwrap_or(other);
            draw.unzoomed.push(color, b.polygon.clone());
            draw.zoomed.push(color.alpha(0.4), b.polygon.clone());
        }

        let panel = Panel::new_builder(Widget::col(vec![
            header(ctx, "Amenities"),
            ColorLegend::row(ctx, food, AmenityType::Food.to_string()),
            ColorLegend::row(ctx, school, AmenityType::School.to_string()),
            ColorLegend::row(ctx, shopping, AmenityType::Shopping.to_string()),
            ColorLegend::row(ctx, other, "other".to_string()),
        ]))
        .aligned_pair(PANEL_PLACEMENT)
        .build(ctx);

        Static {
            panel,
            draw: draw.build(ctx),
            name: "amenities",
        }
    }

    pub fn no_sidewalks(ctx: &mut EventCtx, app: &App) -> Static {
        let mut colorer = ColorDiscrete::new(app, vec![("no sidewalks", Color::RED)]);
        for l in app.primary.map.all_lanes() {
            if l.is_shoulder() && !app.primary.map.get_parent(l.id).is_cycleway() {
                colorer.add_r(l.id.road, "no sidewalks");
            }
        }
        Static::new(
            ctx,
            colorer,
            "no sidewalks",
            "No sidewalks".to_string(),
            Widget::nothing(),
        )
    }

    pub fn blackholes(ctx: &mut EventCtx, app: &App) -> Static {
        let mut colorer = ColorDiscrete::new(
            app,
            vec![
                ("driving blackhole", Color::RED),
                ("biking blackhole", Color::GREEN),
                ("driving + biking blackhole", Color::BLUE),
            ],
        );
        for l in app.primary.map.all_lanes() {
            if l.driving_blackhole && l.biking_blackhole {
                colorer.add_l(l.id, "driving + biking blackhole");
            } else if l.driving_blackhole {
                colorer.add_l(l.id, "driving blackhole");
            } else if l.biking_blackhole {
                colorer.add_l(l.id, "biking blackhole");
            }
        }
        Static::new(
            ctx,
            colorer,
            "blackholes",
            "blackholes".to_string(),
            Widget::nothing(),
        )
    }

    pub fn high_stress(ctx: &mut EventCtx, app: &App) -> Static {
        let mut colorer = ColorDiscrete::new(app, vec![("high stress", app.cs.edits_layer)]);

        for r in app.primary.map.all_roads() {
            if r.high_stress_for_bikes(&app.primary.map, Direction::Fwd)
                || r.high_stress_for_bikes(&app.primary.map, Direction::Back)
            {
                colorer.add_r(r.id, "high stress");
            }
        }

        Static::new(
            ctx,
            colorer,
            "high stress",
            "High stress roads for biking".to_string(),
            Text::from_multiline(vec![
                Line("High stress defined as:"),
                Line("- arterial classification"),
                Line("- no dedicated cycle lane"),
            ])
            .into_widget(ctx),
        )
    }
}
