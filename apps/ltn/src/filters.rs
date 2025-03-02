use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use abstutil::{deserialize_btreemap, serialize_btreemap};
use geom::{Angle, Distance, Line, Speed};
use map_model::{CrossingType, EditRoad, IntersectionID, Map, RoadID, RoutingParams, TurnID};
use widgetry::mapspace::{DrawCustomUnzoomedShapes, PerZoom};
use widgetry::{Color, EventCtx, GeomBatch, RewriteColor};

use crate::render::{colors, Toggle3Zoomed};
use crate::{mut_edits, App};

/// Stored in App per-map state. Before making any changes, call `before_edit`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Edits {
    // We use serialize_btreemap so that save::perma can detect and transform IDs
    #[serde(
        serialize_with = "serialize_btreemap",
        deserialize_with = "deserialize_btreemap"
    )]
    pub roads: BTreeMap<RoadID, RoadFilter>,
    #[serde(
        serialize_with = "serialize_btreemap",
        deserialize_with = "deserialize_btreemap"
    )]
    pub intersections: BTreeMap<IntersectionID, DiagonalFilter>,
    /// For roads with modified directions or speed limits, what's their current state?
    // TODO Misnomer; this includes speed limit changes now too. Not worth a backwards incompatible
    // change right now.
    #[serde(
        serialize_with = "serialize_btreemap",
        deserialize_with = "deserialize_btreemap"
    )]
    pub one_ways: BTreeMap<RoadID, EditRoad>,
    /// For roads with modified speeds, what's their current state?
    #[serde(
        serialize_with = "serialize_btreemap",
        deserialize_with = "deserialize_btreemap"
    )]
    pub speed_limits: BTreeMap<RoadID, Speed>,
    /// One road may have multiple crossings. They're sorted by increasing distance.
    #[serde(
        serialize_with = "serialize_btreemap",
        deserialize_with = "deserialize_btreemap"
    )]
    pub crossings: BTreeMap<RoadID, Vec<Crossing>>,

    /// Edit history is preserved recursively
    #[serde(skip_serializing, skip_deserializing)]
    pub previous_version: Box<Option<Edits>>,
}

/// A filter placed somewhere along a road
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadFilter {
    pub dist: Distance,
    pub filter_type: FilterType,
    pub user_modified: bool,
}

impl RoadFilter {
    pub fn new_by_user(dist: Distance, filter_type: FilterType) -> Self {
        Self {
            dist,
            filter_type,
            user_modified: true,
        }
    }
}

/// Just determines the icon, has no semantics yet
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FilterType {
    NoEntry,
    WalkCycleOnly,
    BusGate,
    SchoolStreet,
}

impl FilterType {
    pub fn svg_path(self) -> &'static str {
        match self {
            FilterType::NoEntry => "system/assets/tools/no_entry.svg",
            FilterType::WalkCycleOnly => "system/assets/tools/modal_filter.svg",
            FilterType::BusGate => "system/assets/tools/bus_gate.svg",
            FilterType::SchoolStreet => "system/assets/tools/school_street.svg",
        }
    }

    pub fn hide_color(self) -> Color {
        match self {
            FilterType::WalkCycleOnly => Color::hex("#0b793a"),
            FilterType::NoEntry => Color::RED,
            FilterType::BusGate => *colors::BUS_ROUTE,
            FilterType::SchoolStreet => Color::hex("#e31017"),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Crossing {
    pub kind: CrossingType,
    pub dist: Distance,
    pub user_modified: bool,
}

/// This logically changes every time an edit occurs. MapName isn't captured here.
#[derive(Default, PartialEq)]
pub struct ChangeKey {
    roads: BTreeMap<RoadID, RoadFilter>,
    intersections: BTreeMap<IntersectionID, DiagonalFilter>,
    one_ways: BTreeMap<RoadID, EditRoad>,
    crossings: BTreeMap<RoadID, Vec<Crossing>>,
}

/// A diagonal filter exists in an intersection. It's defined by two roads (the order is
/// arbitrary). When all of the intersection's roads are sorted in clockwise order, this pair of
/// roads splits the ordering into two groups. Turns in each group are still possible, but not
/// across groups.
///
/// Be careful with `PartialEq` -- see `approx_eq`.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagonalFilter {
    r1: RoadID,
    r2: RoadID,
    i: IntersectionID,
    pub filter_type: FilterType,
    user_modified: bool,

    group1: BTreeSet<RoadID>,
    group2: BTreeSet<RoadID>,
}

impl Edits {
    /// Modify RoutingParams to respect these modal filters
    pub fn update_routing_params(&self, params: &mut RoutingParams) {
        params.avoid_roads.extend(self.roads.keys().cloned());
        for filter in self.intersections.values() {
            params
                .avoid_movements_between
                .extend(filter.avoid_movements_between_roads());
        }
    }

    pub fn allows_turn(&self, t: TurnID) -> bool {
        if let Some(filter) = self.intersections.get(&t.parent) {
            return filter.allows_turn(t.src.road, t.dst.road);
        }
        true
    }

    /// Draw all modal filters
    pub fn draw(&self, ctx: &EventCtx, map: &Map) -> Toggle3Zoomed {
        let mut batch = GeomBatch::new();
        let mut low_zoom = DrawCustomUnzoomedShapes::builder();

        let mut icons = BTreeMap::new();
        for ft in [
            FilterType::NoEntry,
            FilterType::WalkCycleOnly,
            FilterType::BusGate,
            FilterType::SchoolStreet,
        ] {
            icons.insert(ft, GeomBatch::load_svg(ctx, ft.svg_path()));
        }

        for (r, filter) in &self.roads {
            let icon = &icons[&filter.filter_type];
            let rewrite_color = if filter.user_modified {
                RewriteColor::NoOp
            } else {
                RewriteColor::ChangeAlpha(0.7)
            };

            let road = map.get_r(*r);
            if let Ok((pt, road_angle)) = road.center_pts.dist_along(filter.dist) {
                let angle = if filter.filter_type == FilterType::NoEntry {
                    road_angle.rotate_degs(90.0)
                } else {
                    Angle::ZERO
                };

                batch.append(
                    icon.clone()
                        .scale_to_fit_width(road.get_width().inner_meters())
                        .centered_on(pt)
                        .rotate(angle)
                        .color(rewrite_color),
                );

                // TODO Memory intensive
                let icon = icon.clone();
                // TODO They can shrink a bit past their map size
                low_zoom.add_custom(Box::new(move |batch, thickness| {
                    batch.append(
                        icon.clone()
                            .scale_to_fit_width(30.0 * thickness)
                            .centered_on(pt)
                            .rotate(angle)
                            .color(rewrite_color),
                    );
                }));
            }
        }

        for (_, filter) in &self.intersections {
            let icon = &icons[&filter.filter_type];
            let rewrite_color = if filter.user_modified {
                RewriteColor::NoOp
            } else {
                RewriteColor::ChangeAlpha(0.7)
            };

            let line = filter.geometry(map);
            let angle = if filter.filter_type == FilterType::NoEntry {
                line.angle()
            } else {
                Angle::ZERO
            };
            let pt = line.middle().unwrap();

            batch.append(
                icon.clone()
                    .scale_to_fit_width(line.length().inner_meters())
                    .centered_on(pt)
                    .rotate(angle)
                    .color(rewrite_color),
            );

            let icon = icon.clone();
            low_zoom.add_custom(Box::new(move |batch, thickness| {
                // TODO Why is this magic value different than the one above?
                batch.append(
                    icon.clone()
                        .scale(0.4 * thickness)
                        .centered_on(pt)
                        .rotate(angle)
                        .color(rewrite_color),
                );
            }));
        }

        let min_zoom_for_detail = 5.0;
        let step_size = 0.1;
        // TODO Ideally we get rid of Toggle3Zoomed and make DrawCustomUnzoomedShapes handle this
        // medium-zoom case.
        Toggle3Zoomed::new(
            batch.build(ctx),
            low_zoom.build(PerZoom::new(min_zoom_for_detail, step_size)),
        )
    }

    pub fn get_change_key(&self) -> ChangeKey {
        ChangeKey {
            roads: self.roads.clone(),
            intersections: self.intersections.clone(),
            one_ways: self.one_ways.clone(),
            crossings: self.crossings.clone(),
        }
    }
}

impl DiagonalFilter {
    /// The caller must call this in a `before_edit` / `redraw_all_filters` "transaction."
    pub fn cycle_through_alternatives(app: &mut App, i: IntersectionID) {
        let map = &app.per_map.map;
        let mut roads = map.get_i(i).roads.clone();
        // Don't consider non-driveable roads for the 4-way calculation even
        roads.retain(|r| crate::is_driveable(map.get_r(*r), map));

        if roads.len() == 4 {
            // 4-way intersections are the only place where true diagonal filters can be placed
            let alt1 = DiagonalFilter::new(app, i, roads[0], roads[1]);
            let alt2 = DiagonalFilter::new(app, i, roads[1], roads[2]);

            match app.edits().intersections.get(&i) {
                Some(prev) => {
                    if alt1.approx_eq(prev) {
                        mut_edits!(app).intersections.insert(i, alt2);
                    } else if alt2.approx_eq(prev) {
                        mut_edits!(app).intersections.remove(&i);
                    } else {
                        unreachable!()
                    }
                }
                None => {
                    mut_edits!(app).intersections.insert(i, alt1);
                }
            }
        } else if roads.len() > 1 {
            // Diagonal filters elsewhere don't really make sense. They're equivalent to filtering
            // one road. Just cycle through those.

            // But skip roads that're aren't filterable
            roads.retain(|r| {
                let road = map.get_r(*r);
                road.oneway_for_driving().is_none() && !road.is_deadend_for_driving(map)
            });

            // TODO I triggered this case somewhere in Kennington when drawing free-hand. Look for
            // the case and test this case more carefully. Maybe do the filtering earlier.
            if roads.is_empty() {
                return;
            }

            let mut add_filter_to = None;
            if let Some(idx) = roads.iter().position(|r| app.edits().roads.contains_key(r)) {
                mut_edits!(app).roads.remove(&roads[idx]);
                if idx != roads.len() - 1 {
                    add_filter_to = Some(roads[idx + 1]);
                }
            } else {
                add_filter_to = Some(roads[0]);
            }
            if let Some(r) = add_filter_to {
                let road = map.get_r(r);
                let dist = if i == road.src_i {
                    Distance::ZERO
                } else {
                    road.length()
                };
                mut_edits!(app)
                    .roads
                    .insert(r, RoadFilter::new_by_user(dist, app.session.filter_type));
            }
        }
    }

    fn new(app: &App, i: IntersectionID, r1: RoadID, r2: RoadID) -> DiagonalFilter {
        let mut roads = app.per_map.map.get_i(i).roads.clone();
        // Make self.r1 be the first entry
        while roads[0] != r1 {
            roads.rotate_right(1);
        }

        let mut group1 = BTreeSet::new();
        group1.insert(roads.remove(0));
        loop {
            let next = roads.remove(0);
            group1.insert(next);
            if next == r2 {
                break;
            }
        }
        // This is only true for 4-ways...
        assert_eq!(group1.len(), 2);
        assert_eq!(roads.len(), 2);

        DiagonalFilter {
            r1,
            r2,
            i,
            filter_type: app.session.filter_type,
            group1,
            group2: roads.into_iter().collect(),
            // We don't detect existing diagonal filters right now
            user_modified: true,
        }
    }

    /// Physically where is the filter placed?
    pub fn geometry(&self, map: &Map) -> Line {
        let r1 = map.get_r(self.r1);
        let r2 = map.get_r(self.r2);

        // Orient the road to face the intersection
        let pl1 = r1.center_pts.maybe_reverse(r1.src_i == self.i);
        let pl2 = r2.center_pts.maybe_reverse(r2.src_i == self.i);

        // The other combinations of left/right here would produce points or a line across just one
        // road
        let pt1 = pl1.must_shift_right(r1.get_half_width()).last_pt();
        let pt2 = pl2.must_shift_left(r2.get_half_width()).last_pt();
        match Line::new(pt1, pt2) {
            Ok(line) => line,
            // Very rarely, this line is too small. If that happens, just draw something roughly in
            // the right place
            Err(_) => Line::must_new(
                pt1,
                pt1.project_away(r1.get_half_width(), pt1.angle_to(pt2)),
            ),
        }
    }

    pub fn allows_turn(&self, from: RoadID, to: RoadID) -> bool {
        self.group1.contains(&from) == self.group1.contains(&to)
    }

    fn avoid_movements_between_roads(&self) -> Vec<(RoadID, RoadID)> {
        let mut pairs = Vec::new();
        for from in &self.group1 {
            for to in &self.group2 {
                pairs.push((*from, *to));
                pairs.push((*to, *from));
            }
        }
        pairs
    }

    fn approx_eq(&self, other: &DiagonalFilter) -> bool {
        // Careful. At a 4-way intersection, the same filter can be expressed as a different pair of two
        // roads. The (r1, r2) ordering is also arbitrary. cycle_through_alternatives is
        // consistent, though.
        //
        // Note this ignores filter_type.
        (self.r1, self.r2, self.i, &self.group1, &self.group2)
            == (other.r1, other.r2, other.i, &other.group1, &other.group2)
    }
}
