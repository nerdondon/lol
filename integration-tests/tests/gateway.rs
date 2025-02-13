use integration_tests::cluster::*;
use integration_tests::kvs::*;

use lol_core::api::ClusterInfoReq;
use lol_core::gateway;
use lol_core::RaftClient;
use serial_test::serial;
use std::time::Duration;
use tonic::transport::channel::Endpoint;

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_gateway() {
    let env = init_cluster(1);
    let connector = gateway::Connector::new(|id| Endpoint::from(id.clone()));
    let gateway = connector.connect(env.get_node_id(0).parse().unwrap());
    env.start(1, kvs_server(vec![]));
    env.start(2, kvs_server(vec![]));
    Admin::to(0, env.clone()).add_server(1).unwrap();
    Admin::to(0, env.clone()).add_server(2).unwrap();
    tokio::time::sleep(Duration::from_secs(6)).await;

    let mut cli1 = RaftClient::new(gateway.clone());
    let mut cli2 = RaftClient::new(gateway);
    let res = cli1.request_cluster_info(ClusterInfoReq {}).await;
    assert!(res.is_ok());
    let res = cli2.request_cluster_info(ClusterInfoReq {}).await;
    assert!(res.is_ok());

    // When we stop ND0, leadership is transferred to ND1 or ND2.
    // With the gateway, succeeding requests direct to the new leader.
    env.stop(0);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let res = cli1.request_cluster_info(ClusterInfoReq {}).await;
    assert!(res.is_ok());
}
