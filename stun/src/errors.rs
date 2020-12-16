use util::Error;

lazy_static! {
    // ErrAttributeNotFound means that attribute with provided attribute
    // type does not exist in message.
     pub static ref ERR_ATTRIBUTE_NOT_FOUND: Error = Error::new("attribute not found".to_owned());

    // ErrTransactionStopped indicates that transaction was manually stopped.
    pub static ref ERR_TRANSACTION_STOPPED : Error = Error::new("transaction is stopped".to_owned());
    // ErrTransactionNotExists indicates that agent failed to find transaction.
    pub static ref ERR_TRANSACTION_NOT_EXISTS: Error = Error::new("transaction not exists".to_owned());
    // ErrTransactionExists indicates that transaction with same id is already
    // registered.
    pub static ref ERR_TRANSACTION_EXISTS : Error = Error::new("transaction exists with same id".to_owned());
    // ErrAgentClosed indicates that agent is in closed state and is unable
    // to handle transactions.
    pub static ref ERR_AGENT_CLOSED  : Error = Error::new("agent is closed".to_owned());
    // ErrTransactionTimeOut indicates that transaction has reached deadline.
    pub static ref ERR_TRANSACTION_TIME_OUT  : Error = Error::new("transaction is timed out".to_owned());

}
