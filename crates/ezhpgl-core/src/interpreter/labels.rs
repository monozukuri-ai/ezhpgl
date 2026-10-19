use super::*;

impl Interpreter {
    pub(super) fn warning(
        &mut self,
        code: &'static str,
        message: &str,
        span: SourceSpan,
        name: Option<&str>,
        action: &'static str,
    ) -> Result<(), ParseError> {
        if self.diagnostics.iter().any(|d| d.code == code) {
            return Ok(());
        }
        if self.diagnostics.len() >= self.options.parse.limits.max_diagnostics {
            return Err(error(
                "LIMIT_EXCEEDED",
                "Diagnostic limit exceeded",
                span,
                name,
            ));
        }
        self.diagnostics.push(Diagnostic {
            code,
            message: message.into(),
            span,
            command: name.map(Into::into),
            action,
        });
        Ok(())
    }
    fn text_warning(&mut self, c: &Command) -> Result<(), ParseError> {
        self.warning("TEXT_APPROXIMATION","Fallback glyphs and fixed-cell metrics are approximate; dependent positions are marked",c.span,Some(c.name()),"approximate_text")
    }
    pub(super) fn text_command(&mut self, c: &Command, a: &[f64]) -> Result<(), ParseError> {
        let mut p = self.pending();
        let pos = mm(p.state.position);
        if c.name() == "CP" {
            let next = p
                .state
                .text
                .cp(a, pos, &p.state.space)
                .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?;
            p.state.position = Point {
                x: next.x * 40.,
                y: next.y * 40.,
            };
            p.state.position_approximate |= next != pos;
            p.state.path_id += 1;
            self.text_warning(c)?;
        } else {
            p.state
                .text
                .command(c.name(), a, pos)
                .map_err(|(code, message)| error(code, message, c.span, Some(c.name())))?;
        }
        p.state.text_used = true;
        self.commit(p);
        Ok(())
    }
    pub(super) fn label(
        &mut self,
        c: &Command,
        index: usize,
        source: &[u8],
    ) -> Result<(), ParseError> {
        let Payload::Label {
            terminator,
            print_terminator,
        } = c.payload
        else {
            unreachable!()
        };
        let data = &source[c.payload_span.start..c.payload_span.end];
        if data.len() + usize::from(print_terminator) > self.options.geometry_limits.max_label_bytes
        {
            return Err(error(
                "LIMIT_EXCEEDED",
                "Label byte limit exceeded",
                c.span,
                Some("LB"),
            ));
        }
        if !self.state.text.transparent && data.windows(2).any(|b| b == b"\x1bE") {
            return Err(error(
                "UNSUPPORTED_LABEL_CONTROL",
                "Embedded reset inside a normal-mode label is not supported",
                c.span,
                Some("LB"),
            ));
        }
        let mut p = self.pending();
        let start = mm(p.state.position);
        let label = p
            .state
            .text
            .label(
                data,
                c.payload_span,
                terminator,
                print_terminator,
                start,
                &p.state.space,
                self.options
                    .geometry_limits
                    .max_glyphs
                    .saturating_sub(self.glyphs),
            )
            .map_err(|(code, message)| error(code, message, c.span, Some("LB")))?;
        let count = label.glyphs.len();
        let unmapped = !label.unmapped_offsets.is_empty();
        let end = label.end;
        let attrs = p.state.attributes.clone();
        p.state.attributes.line = crate::attributes::LineSpec::default();
        p.state.path_id += 1;
        if !data.is_empty() || print_terminator {
            self.operation(&mut p, Geometry::Label { label }, c, index)?;
        }
        p.state.attributes = attrs;
        p.state.path_id += 1;
        p.state.position = Point {
            x: end.x * 40.,
            y: end.y * 40.,
        };
        p.state.position_approximate |= start != end;
        p.state.text_used = true;
        if !data.is_empty() || print_terminator {
            self.text_warning(c)?;
        }
        if unmapped {
            self.warning("UNMAPPED_TEXT_BYTE","Unmapped or transparent control bytes use replacement glyphs; original bytes are retained",c.span,Some("LB"),"replace_glyph")?;
        }
        self.glyphs += count;
        self.commit(p);
        Ok(())
    }
    pub(super) fn update_carriage(&mut self, c: &Command) {
        let moves = matches!(
            c.name(),
            "AA" | "AR"
                | "AT"
                | "RT"
                | "BZ"
                | "BR"
                | "DF"
                | "IN"
                | "BP"
                | "PA"
                | "PR"
                | "PE"
                | "RO"
        ) || (matches!(c.name(), "PU" | "PD")
            && matches!(&c.payload,Payload::Numbers(a) if !a.is_empty()));
        if moves {
            self.state.text.carriage_return = mm(self.state.position);
        }
    }
    pub(super) fn input_event(&mut self, e: &crate::InputEvent) -> Result<(), ParseError> {
        match e.kind {
            "enter_hpgl" => self.pcl_context = true,
            "enter_hpgl_direct" => self.pcl_context = false,
            "reset" | "end_job" => {
                if !self.page.operations.is_empty() || self.page.omitted_content {
                    self.finish_page(e.span)?;
                }
                self.state = Interpreter::new(self.options).state;
                self.buffer = Arc::default();
                self.buffer_approximate = false;
                self.pcl_context = false;
            }
            "page_break" => {
                self.finish_page(e.span)?;
                self.state.position = Point::default();
                self.state.position_approximate = false;
                self.state.down = false;
                self.state.path_id += 1;
                self.state.text.carriage_return = Point::default();
                self.state.attributes.break_path();
                self.state.polygon = polygon::Cursor::default();
                self.buffer = Arc::default();
                self.buffer_approximate = false;
            }
            "omitted_raster_data" | "omitted_raster_control" => {
                self.warning(
                    "OMITTED_RASTER",
                    "Raster content is framed but not rendered; only HP-GL vectors are retained",
                    e.span,
                    None,
                    "omit_content",
                )?;
                self.page.omitted_content = true;
            }
            "omitted_pcl_text" => {
                self.warning(
                    "OMITTED_PCL_TEXT",
                    "PCL text is not rendered; only HP-GL vectors are retained",
                    e.span,
                    None,
                    "omit_content",
                )?;
                self.page.omitted_content = true;
            }
            _ => {}
        }
        Ok(())
    }
}
