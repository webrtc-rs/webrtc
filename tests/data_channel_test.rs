use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use std::sync::Arc;
use webrtc::data_channel::DataChannel;
use webrtc::peer_connection::*;
use webrtc::runtime::Mutex;
use webrtc::runtime::block_on;

#[derive(Clone)]
struct TestHandler {
    data_channels: Arc<Mutex<Vec<Arc<dyn DataChannel>>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for TestHandler {
    async fn on_data_channel(&self, data_channel: Arc<dyn DataChannel>) {
        self.data_channels.lock().await.push(data_channel);
    }
}

#[test]
fn test_create_data_channel() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let handler = Arc::new(TestHandler {
            data_channels: Arc::new(Mutex::new(Vec::new())),
        });
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Create a data channel
        let _dc = pc.create_data_channel("test", None).await.unwrap();

        //TODO: assert_eq!(dc.label, "test");
    });
}

#[test]
fn test_data_channel_send() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let handler = Arc::new(TestHandler {
            data_channels: Arc::new(Mutex::new(Vec::new())),
        });
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Create a data channel
        let dc = pc.create_data_channel("test", None).await.unwrap();

        // Send should not panic (though it won't actually send without a connection)
        let result = dc.send_text("Hello").await;
        // It's ok if this fails - we don't have a real connection
        let _ = result;
    });
}

#[test]
fn test_multiple_data_channels() {
    block_on(async {
        let config = RTCConfigurationBuilder::new().build();
        let handler = Arc::new(TestHandler {
            data_channels: Arc::new(Mutex::new(Vec::new())),
        });
        let pc = PeerConnectionBuilder::new()
            .with_configuration(config)
            .with_handler(handler)
            .with_udp_addrs(vec!["127.0.0.1:0"])
            .build()
            .await
            .unwrap();

        // Create multiple data channels
        let _dc1 = pc.create_data_channel("channel1", None).await.unwrap();
        let _dc2 = pc.create_data_channel("channel2", None).await.unwrap();
        let _dc3 = pc.create_data_channel("channel3", None).await.unwrap();

        //TODO: assert_eq!(dc1.label, "channel1");
        //TODO: assert_eq!(dc2.label, "channel2");
        //TODO: assert_eq!(dc3.label, "channel3");
    });
}
