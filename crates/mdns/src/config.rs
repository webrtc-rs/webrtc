use std::time::Duration;

// DEFAULT_ADDRESS is the default used by mDNS
// and in most cases should be the address that the
// net.Conn passed to Server is bound to
pub const DEFAULT_ADDRESS: &str = "224.0.0.0:5353";

// Config is used to configure a mDNS client or server.
pub struct Config {
    // query_interval controls how often we sends Queries until we
    // get a response for the requested name
    pub query_interval: Duration,

    // local_names are the names that we will generate answers for
    // when we get questions
    pub local_names: Vec<String>,
    //LoggerFactory logging.LoggerFactory
}
