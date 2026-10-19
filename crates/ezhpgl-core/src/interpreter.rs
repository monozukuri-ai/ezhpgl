use std::sync::Arc;

use crate::attributes::Attributes;
use crate::pe::{Decoder, PeEvent};
use crate::transform::{mm, Space};
use crate::{curves, polygon, ChordTolerance, Contour, Curve};
use crate::{
    Command, CoordinateMode, DeviceProfile, Diagnostic, Geometry, ParseError, ParseOptions,
    Payload, PlotDocument, PlotOperation, PlotPage, PlotState, Point, Provenance, SourceSpan,
};

mod drawing;
mod labels;

#[derive(Clone, Copy, Debug)]
pub struct GeometryLimits {
    pub max_pages: usize,
    pub max_operations: usize,
    pub max_vertices: usize,
    pub max_curve_segments: usize,
    pub max_polygon_vertices: usize,
    pub max_polygon_contours: usize,
    pub max_hatch_lines: usize,
    pub max_label_bytes: usize,
    pub max_glyphs: usize,
}
impl Default for GeometryLimits {
    fn default() -> Self {
        Self {
            max_pages: 10_000,
            max_operations: 1_000_000,
            max_vertices: 2_000_000,
            max_curve_segments: 65_536,
            max_polygon_vertices: 1_000_000,
            max_polygon_contours: 10_000,
            max_hatch_lines: 100_000,
            max_label_bytes: 65_536,
            max_glyphs: 200_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InterpretOptions {
    pub parse: ParseOptions,
    pub preserve_commands: bool,
    pub geometry_limits: GeometryLimits,
    pub profile: DeviceProfile,
}

fn error(code: &'static str, message: &str, span: SourceSpan, name: Option<&str>) -> ParseError {
    ParseError {
        diagnostic: Diagnostic {
            code,
            message: message.into(),
            span,
            command: name.map(Into::into),
            action: "stop_interpretation",
        },
    }
}

pub fn parse(data: &[u8], options: InterpretOptions) -> Result<PlotDocument, ParseError> {
    if data.len() > options.parse.limits.max_input_bytes {
        return Err(error(
            "LIMIT_EXCEEDED",
            "Input byte limit exceeded",
            SourceSpan {
                start: 0,
                end: data.len(),
            },
            None,
        ));
    }
    parse_owned(Arc::from(data), options)
}

pub fn parse_owned(
    source: Arc<[u8]>,
    options: InterpretOptions,
) -> Result<PlotDocument, ParseError> {
    if !options.profile.validate() {
        return Err(error(
            "INVALID_PROFILE",
            "Profile extents must be finite, positive and within the supported range",
            SourceSpan::default(),
            None,
        ));
    }
    // Merge the first lexical failure into command order. Raw recovery does not
    // authorize semantic recovery, and earlier semantic failures take precedence.
    let raw = Arc::new(crate::lexer::parse_input(
        source,
        ParseOptions {
            strict: false,
            ..options.parse
        },
        true,
    )?);
    let first_lexical = raw.diagnostics.first();
    let mut interpreter = Interpreter::new(options);
    let mut failure = None;
    let mut interpreted_commands = 0;
    let mut event_index = 0;
    while interpreted_commands < raw.commands.len() || event_index < raw.input_events.len() {
        let command = raw.commands.get(interpreted_commands);
        let event = raw.input_events.get(event_index);
        let use_event = event.is_some_and(|e| command.is_none_or(|c| e.span.start < c.span.start));
        let span = if use_event {
            event.unwrap().span
        } else {
            command.unwrap().span
        };
        if let Some(d) = first_lexical.filter(|d| d.span.start < span.end) {
            failure = Some(ParseError {
                diagnostic: d.clone(),
            });
            break;
        }
        let result = if use_event {
            interpreter.input_event(event.unwrap())
        } else {
            interpreter.execute(command.unwrap(), interpreted_commands, &raw.source)
        };
        if let Err(e) = result {
            failure = Some(e);
            break;
        }
        if use_event {
            event_index += 1;
        } else {
            interpreter.update_carriage(command.unwrap());
            interpreted_commands += 1;
        }
    }
    if failure.is_none() {
        failure = first_lexical.map(|d| ParseError {
            diagnostic: d.clone(),
        });
    }
    if failure.is_none() && interpreter.state.polygon.active {
        failure = Some(error(
            "UNCLOSED_POLYGON",
            "Polygon mode is still active at end of input",
            SourceSpan {
                start: raw.source.len(),
                end: raw.source.len(),
            },
            Some("PM"),
        ));
    }
    if let Some(mut failure) = failure {
        failure.diagnostic.action = "stop_interpretation";
        if options.parse.strict || failure.diagnostic.code == "LIMIT_EXCEEDED" {
            return Err(failure);
        }
        if interpreter.diagnostics.len() >= options.parse.limits.max_diagnostics {
            return Err(error(
                "LIMIT_EXCEEDED",
                "Diagnostic limit exceeded",
                failure.diagnostic.span,
                None,
            ));
        }
        interpreter.diagnostics.push(failure.diagnostic);
    }
    if !interpreter.page.operations.is_empty()
        || interpreter.page.omitted_content
        || interpreter.pages.is_empty()
    {
        interpreter.finish_page(SourceSpan {
            start: raw.source.len(),
            end: raw.source.len(),
        })?;
    }
    let state = interpreter.state;
    let diagnostics = interpreter.diagnostics;
    let transform = state
        .space
        .snapshot()
        .map_err(|(code, message)| error(code, message, SourceSpan::default(), None))?;
    Ok(PlotDocument {
        schema_version: 1,
        profile: "generic-plotter",
        device_profile: options.profile,
        units: "mm",
        completeness: if diagnostics.is_empty() {
            "complete"
        } else {
            "partial"
        },
        source_size: raw.source.len(),
        interpreted_commands,
        pages: interpreter.pages,
        diagnostics,
        input_events: raw.input_events[..event_index].to_vec(),
        final_state: PlotState {
            position: mm(state.position),
            coordinate_mode: state.mode,
            pen: state.pen,
            pen_down: state.down,
            transform,
            polygon_mode: state.polygon.active,
            text: state.text_used.then_some(state.text),
            position_approximate: state.position_approximate,
        },
        raw: options.preserve_commands.then_some(raw),
    })
}

fn coordinate_ok(v: f64) -> bool {
    v.is_finite() && (-8_388_608.0..=8_388_607.0).contains(&v)
}

#[derive(Clone)]
struct State {
    position: Point,
    mode: CoordinateMode,
    pen: u32,
    down: bool,
    path_id: usize,
    space: Space,
    attributes: Attributes,
    chord_mode: u8,
    polygon: polygon::Cursor,
    text: crate::TextState,
    text_used: bool,
    position_approximate: bool,
}
struct Pending {
    state: State,
    operations: Vec<PlotOperation>,
    vertices: usize,
    buffered: Vec<polygon::Append>,
}
struct Interpreter {
    state: State,
    operations: usize,
    vertices: usize,
    pages: Vec<PlotPage>,
    page: PlotPage,
    options: InterpretOptions,
    buffer: Arc<Vec<Contour>>,
    buffer_approximate: bool,
    glyphs: usize,
    diagnostics: Vec<Diagnostic>,
    pcl_context: bool,
}

impl Interpreter {
    fn new(options: InterpretOptions) -> Self {
        Self {
            state: State {
                position: Point::default(),
                mode: CoordinateMode::Absolute,
                pen: 0,
                down: false,
                path_id: 0,
                space: Space::new(options.profile),
                attributes: Attributes::default(),
                chord_mode: 0,
                polygon: polygon::Cursor::default(),
                text: crate::TextState::default(),
                text_used: false,
                position_approximate: false,
            },
            operations: 0,
            vertices: 0,
            pages: Vec::new(),
            page: Self::empty_page(options.profile),
            options,
            buffer: Arc::default(),
            buffer_approximate: false,
            glyphs: 0,
            diagnostics: Vec::new(),
            pcl_context: false,
        }
    }
    fn empty_page(profile: DeviceProfile) -> PlotPage {
        PlotPage {
            paper_size_mm: profile.paper_size_mm,
            plot_size_mm: profile.plot_size_mm,
            ..Default::default()
        }
    }
    fn finish_page(&mut self, span: SourceSpan) -> Result<(), ParseError> {
        if self.pages.len() >= self.options.geometry_limits.max_pages {
            return Err(error("LIMIT_EXCEEDED", "Page limit exceeded", span, None));
        }
        self.page.index = self.pages.len();
        self.pages.push(std::mem::replace(
            &mut self.page,
            Self::empty_page(self.options.profile),
        ));
        Ok(())
    }
    fn pending(&self) -> Pending {
        Pending {
            state: self.state.clone(),
            operations: Vec::new(),
            vertices: 0,
            buffered: Vec::new(),
        }
    }
    fn commit(&mut self, pending: Pending) {
        if !pending.buffered.is_empty() {
            self.buffer_approximate |= pending.state.polygon.approximate;
            let buffer = Arc::make_mut(&mut self.buffer);
            for append in pending.buffered {
                append.apply(buffer);
            }
        }
        self.state = pending.state;
        self.operations += pending.operations.len();
        self.vertices += pending.vertices;
        for op in pending.operations {
            self.page.push(op);
        }
    }
    fn emit(
        &self,
        p: &mut Pending,
        end: Option<Point>,
        dot: bool,
        c: &Command,
        index: usize,
    ) -> Result<(), ParseError> {
        if p.state.pen == 0 {
            return Ok(());
        }
        p.state.attributes.begin_stroke(mm(p.state.position));
        let append = end.is_some()
            && p.operations
                .last()
                .is_some_and(|op| op.path_id == p.state.path_id);
        let needed = if append {
            1
        } else {
            1 + usize::from(end.is_some())
        };
        let limits = self.options.geometry_limits;
        if self.pages.len() >= limits.max_pages
            || (!append
                && p.operations.len() >= limits.max_operations.saturating_sub(self.operations))
            || needed
                > limits
                    .max_vertices
                    .saturating_sub(self.vertices)
                    .saturating_sub(p.vertices)
        {
            return Err(error(
                "LIMIT_EXCEEDED",
                "Page, drawing operation or vertex limit exceeded",
                c.span,
                Some(c.name()),
            ));
        }
        if append {
            let Geometry::Polyline { points, .. } = &mut p.operations.last_mut().unwrap().geometry
            else {
                unreachable!()
            };
            points.push(mm(end.unwrap()));
        } else {
            let mut points = vec![mm(p.state.position)];
            if let Some(end) = end {
                points.push(mm(end));
            }
            p.operations.push(PlotOperation {
                approximate: p.state.position_approximate,
                geometry: Geometry::Polyline {
                    points,
                    start_dot: dot,
                },
                style: p
                    .state
                    .attributes
                    .snapshot(p.state.pen, &p.state.space)
                    .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?,
                source: Provenance {
                    command_index: index,
                    command: c.name().into(),
                    span: c.span,
                },
                path_id: p.state.path_id,
                clip: p
                    .state
                    .space
                    .clip()
                    .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?,
            });
        }
        p.vertices += needed;
        if let Some(end) = end {
            Self::advance_phase(p, mm(p.state.position), mm(end))?;
        }
        Ok(())
    }
    fn move_pair(
        &self,
        p: &mut Pending,
        point: Point,
        relative: bool,
        dot: bool,
        c: &Command,
        index: usize,
    ) -> Result<(), ParseError> {
        let fail = || {
            error(
                "COORDINATE_RANGE",
                "Input or transformed coordinate exceeds the generic profile range",
                c.span,
                Some(c.name()),
            )
        };
        if !coordinate_ok(point.x) || !coordinate_ok(point.y) {
            return Err(fail());
        }
        let mapped = p
            .state
            .space
            .map(point, relative)
            .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?;
        let target = if relative {
            Point {
                x: p.state.position.x + mapped.x,
                y: p.state.position.y + mapped.y,
            }
        } else {
            mapped
        };
        if !coordinate_ok(target.x) || !coordinate_ok(target.y) {
            return Err(fail());
        }
        if p.state.polygon.active {
            self.buffer_path(
                p,
                vec![mm(p.state.position), mm(target)],
                None,
                p.state.down,
                true,
                c,
                index,
            )?;
        } else if p.state.down {
            self.emit(p, Some(target), dot, c, index)?;
        }
        p.state.position = target;
        if !relative {
            p.state.position_approximate = false;
        }
        Ok(())
    }
    fn execute_pe(&mut self, c: &Command, index: usize, source: &[u8]) -> Result<(), ParseError> {
        let span = c.payload_span;
        let mut decoder = Decoder::new(
            &source[span.start..span.end],
            span.start,
            self.options.parse.limits.max_arguments_per_command,
        );
        let mut pending = self.pending();
        while let Some(event) = decoder.next()? {
            match event {
                PeEvent::Pen(pen) => {
                    if !pending.state.polygon.active {
                        pending.state.pen = pending.state.attributes.select(pen);
                        pending.state.path_id += 1;
                        pending.state.attributes.break_path();
                    }
                }
                PeEvent::Move {
                    point,
                    absolute,
                    up,
                } => {
                    if up {
                        pending.state.path_id += 1;
                    }
                    pending.state.down = !up;
                    self.move_pair(&mut pending, point, !absolute, true, c, index)?;
                }
            }
        }
        self.commit(pending);
        Ok(())
    }
    fn execute(&mut self, c: &Command, index: usize, source: &[u8]) -> Result<(), ParseError> {
        let fail = |code, message| error(code, message, c.span, Some(c.name()));
        if self.state.polygon.active
            && !matches!(
                c.name(),
                "AA" | "AR"
                    | "AT"
                    | "RT"
                    | "BR"
                    | "BZ"
                    | "CI"
                    | "CO"
                    | "PA"
                    | "PR"
                    | "PU"
                    | "PD"
                    | "PE"
                    | "PM"
                    | "IN"
                    | "DF"
                    | "BP"
                    | "EP"
                    | "FP"
            )
        {
            return Err(fail(
                "INVALID_POLYGON_COMMAND",
                "Command is not allowed while polygon mode is active",
            ));
        }
        if matches!(
            c.payload,
            Payload::Comment | Payload::LabelTerminator { .. }
        ) {
            return Ok(());
        }
        if matches!(c.payload, Payload::EncodedPolyline) {
            return self.execute_pe(c, index, source);
        }
        if matches!(c.payload, Payload::Label { .. }) {
            return self.label(c, index, source);
        }
        let numbers = match &c.payload {
            Payload::Numbers(n) => n,
            _ => {
                return Err(fail(
                    "UNSUPPORTED_COMMAND",
                    "Command semantics are not supported",
                ))
            }
        };
        match c.name() {
            "IN" | "DF" | "BP" => {
                if !numbers.is_empty() {
                    return Err(fail(
                        "INVALID_ARGUMENTS",
                        "This command requires no arguments",
                    ));
                }
                if c.name() == "BP"
                    && (!self.page.operations.is_empty() || self.page.omitted_content)
                {
                    self.finish_page(c.span)?;
                }
                self.state.mode = CoordinateMode::Absolute;
                self.state.polygon = polygon::Cursor::default();
                self.buffer = Arc::default();
                self.buffer_approximate = false;
                self.state.chord_mode = 0;
                self.state.text = crate::TextState::default();
                self.state.text_used = false;
                if c.name() == "DF" {
                    self.state.space.defaults();
                    self.state.attributes.defaults();
                } else {
                    self.state.space.initialize();
                    self.state.position = Point::default();
                    self.state.position_approximate = false;
                    self.state.down = false;
                    self.state.attributes = Attributes::default();
                }
                self.state.path_id += 1;
            }
            "SP" => {
                if numbers.len() > 1 {
                    return Err(fail("INVALID_ARGUMENTS", "SP accepts zero or one argument"));
                }
                let pen = numbers.first().map_or(0., |n| n.value);
                if !(0.0..=8_388_607.).contains(&pen) {
                    return Err(fail(
                        "INVALID_PEN",
                        "Generic pen IDs must be between 0 and 8388607",
                    ));
                }
                self.state.pen = self.state.attributes.select(pen.round() as u32);
                self.state.path_id += 1;
                self.state.attributes.break_path();
            }
            "PG" => {
                if numbers.len() > 1 {
                    return Err(fail("INVALID_ARGUMENTS", "PG accepts zero or one argument"));
                }
                if source.get(c.span.end.saturating_sub(1)) != Some(&b';') {
                    return Err(fail(
                        "INVALID_TERMINATOR",
                        "PG requires a semicolon in this profile",
                    ));
                }
                if self.pcl_context {
                    return Ok(());
                }
                if !numbers.is_empty()
                    || !self.page.operations.is_empty()
                    || self.page.omitted_content
                {
                    self.finish_page(c.span)?;
                }
                self.state.position = Point::default();
                self.state.position_approximate = false;
                self.state.text.carriage_return = Point::default();
                self.state.down = false;
                self.state.path_id += 1;
                self.state.attributes.break_path();
                self.state.polygon = polygon::Cursor::default();
                self.buffer = Arc::default();
                self.buffer_approximate = false;
            }
            "IP" | "IR" | "SC" | "RO" | "IW" => {
                let mut space = self.state.space.clone();
                let args: Vec<f64> = numbers.iter().map(|n| n.value).collect();
                space
                    .command(c.name(), &args)
                    .map_err(|(code, message)| fail(code, message))?;
                let mut attrs = self.state.attributes.clone();
                if c.name() == "SC" && !space.is_scaled() {
                    attrs
                        .freeze_hatch(&self.state.space)
                        .map_err(|(code, message)| fail(code, message))?;
                }
                self.state.space = space;
                self.state.attributes = attrs;
                self.state.attributes.break_path();
                self.state.path_id += 1;
            }
            "PA" | "PR" | "PU" | "PD" => {
                if numbers.len() % 2 != 0 {
                    return Err(fail("INVALID_ARGUMENTS", "Coordinates require X,Y pairs"));
                }
                let mut pending = self.pending();
                pending.state.mode = match c.name() {
                    "PA" => CoordinateMode::Absolute,
                    "PR" => CoordinateMode::Relative,
                    _ => pending.state.mode,
                };
                pending.state.down = match c.name() {
                    "PD" => true,
                    "PU" => false,
                    _ => pending.state.down,
                };
                if c.name() == "PU" {
                    pending.state.path_id += 1;
                }
                let relative = pending.state.mode == CoordinateMode::Relative;
                if numbers.is_empty() && c.name() == "PD" && !pending.state.polygon.active {
                    self.emit(&mut pending, None, true, c, index)?;
                }
                for pair in numbers.chunks_exact(2) {
                    self.move_pair(
                        &mut pending,
                        Point {
                            x: pair[0].value,
                            y: pair[1].value,
                        },
                        relative,
                        c.name() == "PD",
                        c,
                        index,
                    )?;
                }
                self.commit(pending);
            }
            "CI" | "AA" | "AR" | "AT" | "RT" | "BZ" | "BR" | "EA" | "ER" | "RA" | "RR" | "EW"
            | "WG" | "PM" | "FP" | "EP" | "CT" => {
                let args: Vec<f64> = numbers.iter().map(|n| n.value).collect();
                self.drawing(c, index, &args)?;
            }
            "SI" | "SR" | "DI" | "DR" | "LO" | "SL" | "ES" | "DV" | "SD" | "AD" | "SS" | "SA"
            | "TD" | "SB" | "CF" | "CP" => {
                let args: Vec<_> = numbers.iter().map(|n| n.value).collect();
                self.text_command(c, &args)?;
            }
            "PW" | "WU" | "PC" | "NP" | "LT" | "LA" | "FT" => {
                let args: Vec<f64> = numbers.iter().map(|n| n.value).collect();
                let mut attrs = self.state.attributes.clone();
                let mut pen = self.state.pen;
                attrs
                    .command(
                        c.name(),
                        &args,
                        &self.state.space,
                        self.state.position,
                        &mut pen,
                    )
                    .map_err(|(code, message)| fail(code, message))?;
                self.state.attributes = attrs;
                self.state.pen = pen;
                self.state.path_id += 1;
            }
            _ => {
                return Err(fail(
                    "UNSUPPORTED_COMMAND",
                    "Command semantics are not supported",
                ))
            }
        }
        Ok(())
    }
}
