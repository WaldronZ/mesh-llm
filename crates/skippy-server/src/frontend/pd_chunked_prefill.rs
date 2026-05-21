use super::*;

pub(super) const PD_CHUNKED_PREFILL_PROTOCOL_VERSION: &str = "pd-prefill-chunked/1";
pub(super) const PD_CHUNKED_PREFILL_CAPABILITY: &str = "chunked-prefill";
const PD_CHUNKED_PREFILL_TELEMETRY_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PdChunkedPrefillConfig {
    pub(super) chunk_size: usize,
    pub(super) max_prefill_batch: usize,
}

impl PdChunkedPrefillConfig {
    pub(super) fn new(chunk_size: usize, max_prefill_batch: usize) -> Result<Self> {
        if chunk_size == 0 {
            bail!("--pd-prefill-chunk-size must be greater than zero");
        }
        if max_prefill_batch == 0 {
            bail!("--pd-max-prefill-batch must be greater than zero");
        }
        if chunk_size > max_prefill_batch {
            bail!("--pd-prefill-chunk-size must be less than or equal to --pd-max-prefill-batch");
        }
        Ok(Self {
            chunk_size,
            max_prefill_batch,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PdPrefillChunkRange {
    pub(super) index: usize,
    pub(super) start_position: usize,
    pub(super) end_position: usize,
    pub(super) token_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PdChunkedPrefillPlan {
    pub(super) total_prefill_tokens: usize,
    pub(super) chunk_size: usize,
    pub(super) chunks: Vec<PdPrefillChunkRange>,
}

impl PdChunkedPrefillPlan {
    pub(super) fn new(total_prefill_tokens: usize, config: PdChunkedPrefillConfig) -> Result<Self> {
        if config.chunk_size > config.max_prefill_batch {
            bail!("chunk size exceeds max prefill batch");
        }
        let mut chunks = Vec::new();
        let mut start_position = 0usize;
        while start_position < total_prefill_tokens {
            let remaining = total_prefill_tokens - start_position;
            let token_count = remaining.min(config.chunk_size);
            let end_position = start_position + token_count;
            chunks.push(PdPrefillChunkRange {
                index: chunks.len(),
                start_position,
                end_position,
                token_count,
            });
            start_position = end_position;
        }
        Ok(Self {
            total_prefill_tokens,
            chunk_size: config.chunk_size,
            chunks,
        })
    }

    pub(super) fn provenance(&self) -> PdChunkedPrefillManifestProvenance {
        PdChunkedPrefillManifestProvenance {
            chunked_prefill: true,
            protocol_version: PD_CHUNKED_PREFILL_PROTOCOL_VERSION,
            capability: PD_CHUNKED_PREFILL_CAPABILITY,
            chunk_count: self.chunks.len(),
            chunk_size: self.chunk_size,
            total_prefill_tokens: self.total_prefill_tokens,
            final_decode_start_position: self.total_prefill_tokens,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PdChunkedPrefillManifestProvenance {
    pub(super) chunked_prefill: bool,
    pub(super) protocol_version: &'static str,
    pub(super) capability: &'static str,
    pub(super) chunk_count: usize,
    pub(super) chunk_size: usize,
    pub(super) total_prefill_tokens: usize,
    pub(super) final_decode_start_position: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PdChunkedPrefillTelemetry {
    pub(super) provenance: PdChunkedPrefillManifestProvenance,
    pub(super) chunk_tokens: Vec<usize>,
    pub(super) chunk_prefill_ms: Vec<f64>,
    pub(super) total_prefill_ms: f64,
}

impl PdChunkedPrefillTelemetry {
    pub(super) fn from_plan(
        plan: &PdChunkedPrefillPlan,
        chunk_prefill_ms: Vec<f64>,
        total_prefill_ms: f64,
    ) -> Self {
        Self {
            provenance: plan.provenance(),
            chunk_tokens: plan.chunks.iter().map(|chunk| chunk.token_count).collect(),
            chunk_prefill_ms,
            total_prefill_ms,
        }
    }

    pub(super) fn bounded_chunk_tokens(&self) -> Vec<usize> {
        bounded_list(&self.chunk_tokens)
    }

    pub(super) fn bounded_chunk_prefill_ms(&self) -> Vec<f64> {
        bounded_list(&self.chunk_prefill_ms)
    }

    pub(super) fn chunk_tokens_truncated(&self) -> bool {
        self.chunk_tokens.len() > PD_CHUNKED_PREFILL_TELEMETRY_LIMIT
    }

    pub(super) fn chunk_prefill_ms_truncated(&self) -> bool {
        self.chunk_prefill_ms.len() > PD_CHUNKED_PREFILL_TELEMETRY_LIMIT
    }
}

fn bounded_list<T: Copy>(values: &[T]) -> Vec<T> {
    values
        .iter()
        .copied()
        .take(PD_CHUNKED_PREFILL_TELEMETRY_LIMIT)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PdChunkedPrefillTerminal {
    Completed,
    Rejected,
    Error,
    #[cfg(test)]
    Timeout,
    #[cfg(test)]
    Cancelled,
}

pub(super) struct PdChunkedPrefillSession {
    plan: PdChunkedPrefillPlan,
    next_chunk_index: usize,
    expected_position: usize,
    terminal: Option<PdChunkedPrefillTerminal>,
    cleaned_up: bool,
    #[cfg(test)]
    cleanup_failure_label: Option<&'static str>,
}

impl PdChunkedPrefillSession {
    pub(super) fn new(plan: PdChunkedPrefillPlan) -> Self {
        Self {
            plan,
            next_chunk_index: 0,
            expected_position: 0,
            terminal: None,
            cleaned_up: false,
            #[cfg(test)]
            cleanup_failure_label: None,
        }
    }

    pub(super) fn acknowledge(&mut self, chunk: PdPrefillChunkRange) -> Result<()> {
        self.ensure_active()?;
        let expected = self
            .plan
            .chunks
            .get(self.next_chunk_index)
            .copied()
            .ok_or_else(|| anyhow!("unexpected chunk ACK after final chunk"))?;
        if chunk != expected || chunk.start_position != self.expected_position {
            self.fail_and_cleanup(PdChunkedPrefillTerminal::Rejected);
            bail!("chunked prefill position continuity check failed");
        }
        self.expected_position = chunk.end_position;
        self.next_chunk_index += 1;
        if self.next_chunk_index == self.plan.chunks.len() {
            self.terminal = Some(PdChunkedPrefillTerminal::Completed);
        }
        Ok(())
    }

    pub(super) fn error_chunk(&mut self) {
        self.fail_and_cleanup(PdChunkedPrefillTerminal::Error);
    }

    #[cfg(test)]
    pub(super) fn timeout(&mut self) {
        self.fail_and_cleanup(PdChunkedPrefillTerminal::Timeout);
    }

    #[cfg(test)]
    pub(super) fn cancel(&mut self) {
        self.fail_and_cleanup(PdChunkedPrefillTerminal::Cancelled);
    }

    pub(super) fn cleanup(&mut self) {
        self.cleaned_up = true;
    }

    #[cfg(test)]
    pub(super) fn record_cleanup_failure(&mut self) {
        self.cleanup_failure_label = Some("cleanup_failed");
    }

    pub(super) fn can_export(&self) -> bool {
        self.terminal == Some(PdChunkedPrefillTerminal::Completed)
            && self.expected_position == self.plan.total_prefill_tokens
    }

    #[cfg(test)]
    pub(super) fn cleaned_up(&self) -> bool {
        self.cleaned_up
    }

    #[cfg(test)]
    pub(super) fn expected_position(&self) -> usize {
        self.expected_position
    }

    #[cfg(test)]
    pub(super) fn cleanup_failure_label(&self) -> Option<&'static str> {
        self.cleanup_failure_label
    }

    fn ensure_active(&self) -> Result<()> {
        if self.terminal.is_some() {
            bail!("chunked prefill session is already terminal");
        }
        Ok(())
    }

    fn fail_and_cleanup(&mut self, terminal: PdChunkedPrefillTerminal) {
        self.terminal = Some(terminal);
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PdChunkedPrefillConfig {
        PdChunkedPrefillConfig::new(1800, 1800).unwrap()
    }

    #[test]
    fn planner_splits_4k_and_8k_into_safe_ranges() {
        let plan = PdChunkedPrefillPlan::new(4000, config()).unwrap();
        assert_eq!(
            plan.chunks
                .iter()
                .map(|chunk| chunk.token_count)
                .collect::<Vec<_>>(),
            vec![1800, 1800, 400]
        );
        assert_eq!(plan.chunks[1].start_position, 1800);
        assert_eq!(plan.chunks[2].end_position, 4000);

        let plan = PdChunkedPrefillPlan::new(8000, config()).unwrap();
        assert_eq!(plan.chunks.len(), 5);
        assert_eq!(plan.chunks.last().unwrap().token_count, 800);
        assert_eq!(plan.chunks.last().unwrap().end_position, 8000);
    }

    #[test]
    fn planner_handles_exact_boundary_and_rejects_oversized_chunk() {
        let plan = PdChunkedPrefillPlan::new(3600, config()).unwrap();
        assert_eq!(plan.chunks.len(), 2);
        assert_eq!(plan.chunks[1].token_count, 1800);

        let error = PdChunkedPrefillConfig::new(2048, 1800).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--pd-prefill-chunk-size must be less than or equal"),
            "{error:?}"
        );
    }

    #[test]
    fn state_machine_preserves_position_continuity() {
        let plan = PdChunkedPrefillPlan::new(4000, config()).unwrap();
        let mut session = PdChunkedPrefillSession::new(plan.clone());
        for chunk in plan.chunks {
            session.acknowledge(chunk).unwrap();
        }
        assert_eq!(session.expected_position(), 4000);
        assert!(session.can_export());
        assert!(!session.cleaned_up());
    }

    #[test]
    fn state_machine_fails_closed_on_position_mismatch() {
        let plan = PdChunkedPrefillPlan::new(4000, config()).unwrap();
        let mut session = PdChunkedPrefillSession::new(plan.clone());
        let mut bad = plan.chunks[0];
        bad.end_position += 1;
        let error = session.acknowledge(bad).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("position continuity check failed"),
            "{error:?}"
        );
        assert!(!session.can_export());
        assert!(session.cleaned_up());
    }

    #[test]
    fn state_machine_terminal_errors_cleanup_and_block_export() {
        for terminal in [
            PdChunkedPrefillTerminal::Error,
            PdChunkedPrefillTerminal::Timeout,
            PdChunkedPrefillTerminal::Cancelled,
        ] {
            let plan = PdChunkedPrefillPlan::new(4000, config()).unwrap();
            let mut session = PdChunkedPrefillSession::new(plan);
            match terminal {
                PdChunkedPrefillTerminal::Error => session.error_chunk(),
                PdChunkedPrefillTerminal::Timeout => session.timeout(),
                PdChunkedPrefillTerminal::Cancelled => session.cancel(),
                _ => unreachable!(),
            }
            assert!(!session.can_export());
            assert!(session.cleaned_up());
        }
    }

    #[test]
    fn state_machine_records_sanitized_cleanup_failure_label() {
        let plan = PdChunkedPrefillPlan::new(4000, config()).unwrap();
        let mut session = PdChunkedPrefillSession::new(plan);
        session.error_chunk();
        session.record_cleanup_failure();

        assert_eq!(session.cleanup_failure_label(), Some("cleanup_failed"));
        assert!(!session.cleanup_failure_label().unwrap().contains('/'));
        assert!(!session.cleanup_failure_label().unwrap().contains("http"));
    }

    #[test]
    fn provenance_and_telemetry_are_bounded_and_sanitized() {
        let plan = PdChunkedPrefillPlan::new(4000, config()).unwrap();
        let telemetry = PdChunkedPrefillTelemetry::from_plan(&plan, vec![1.0, 2.0, 3.0], 6.0);

        assert_eq!(
            telemetry.provenance.protocol_version,
            "pd-prefill-chunked/1"
        );
        assert_eq!(telemetry.provenance.capability, "chunked-prefill");
        assert!(telemetry.provenance.chunked_prefill);
        assert_eq!(telemetry.provenance.chunk_count, 3);
        assert_eq!(telemetry.provenance.final_decode_start_position, 4000);
        assert_eq!(telemetry.bounded_chunk_tokens(), vec![1800, 1800, 400]);
        assert!(!telemetry.chunk_tokens_truncated());
    }
}
