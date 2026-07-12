//! Builder for `MockNcp` instances.

use wisun_types::{DriverState, NcpState};

use crate::config::{CapabilitySet, MockConfig};
use crate::failure::FailureRule;
use crate::mock_ncp::MockNcp;
use crate::pty_transport::DuplexTransport;
use crate::topology::MockTopology;

/// Builder for constructing `MockNcp` instances. The default configuration
/// creates a router-capable NCP over an in-memory `DuplexTransport`.
pub struct MockNcpBuilder<T: dcu_serial::transport::Transport + Unpin> {
    config: MockConfig,
    topology: MockTopology,
    failure_rules: Vec<FailureRule>,
    initial_ncp_state: NcpState,
    initial_driver_state: DriverState,
    capabilities: CapabilitySet,
    _transport: std::marker::PhantomData<T>,
}

impl Default for MockNcpBuilder<DuplexTransport> {
    fn default() -> Self {
        Self::new()
    }
}

impl MockNcpBuilder<DuplexTransport> {
    /// Default builder for the common in-memory test case.
    pub fn new() -> Self {
        Self {
            config: MockConfig::default(),
            topology: MockTopology::new(),
            failure_rules: Vec::new(),
            initial_ncp_state: NcpState::Uninitialized,
            initial_driver_state: DriverState::Initializing,
            capabilities: CapabilitySet::router(),
            _transport: std::marker::PhantomData,
        }
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.config.ncp_version = version.to_string();
        self
    }

    pub fn with_address(mut self, eui64: wisun_types::Eui64) -> Self {
        self.config.hardware_address = eui64;
        self
    }

    pub fn with_topology(mut self, topology: MockTopology) -> Self {
        self.topology = topology;
        self
    }

    pub fn with_failure(mut self, rule: FailureRule) -> Self {
        self.failure_rules.push(rule);
        self
    }

    pub fn with_capabilities(mut self, caps: CapabilitySet) -> Self {
        self.capabilities = caps;
        self
    }

    /// Build a `MockNcp` using an internal `DuplexTransport` pair.
    /// Returns the mock and the daemon-side transport.
    pub fn build(
        self,
    ) -> (MockNcp<DuplexTransport>, DuplexTransport) {
        let (daemon_raw, mock_raw) = tokio::io::duplex(crate::pty_transport::DUPLEX_BUFFER_SIZE);
        let mock = MockNcp::new(
            DuplexTransport(mock_raw),
            self.config,
            self.topology,
            self.capabilities,
            self.failure_rules,
            self.initial_ncp_state,
            self.initial_driver_state,
        )
        .expect("MockNcp construction should not fail");
        (mock, DuplexTransport(daemon_raw))
    }
}

impl<T: dcu_serial::transport::Transport + Unpin> MockNcpBuilder<T> {
    /// Build a mock over an existing transport.
    pub fn build_with_transport(self, transport: T) -> MockNcp<T> {
        MockNcp::new(
            transport,
            self.config,
            self.topology,
            self.capabilities,
            self.failure_rules,
            self.initial_ncp_state,
            self.initial_driver_state,
        )
        .expect("MockNcp construction should not fail")
    }
}
