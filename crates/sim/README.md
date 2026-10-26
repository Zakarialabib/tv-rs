# tv-rs-sim

TCP Mock Server for commercial display protocols (Samsung MDC on TCP 1515 and Philips SICP on TCP 5000) for hardware-free CI testing.

## Example

```no_run
use tv_rs_sim::{MockMdcServer, MockSicpServer};
use tv_rs_mdc::MdcClient;
use tv_rs_sicp::SicpClient;

#[tokio::main]
async fn main() {
    let mdc_server = MockMdcServer::start("127.0.0.1:0").await.unwrap();
    let mdc_client = MdcClient::new(mdc_server.local_addr().to_string());

    let sicp_server = MockSicpServer::start("127.0.0.1:0").await.unwrap();
    let sicp_client = SicpClient::new(sicp_server.local_addr().to_string());
}
```
