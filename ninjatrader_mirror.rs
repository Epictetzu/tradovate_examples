use tradovate_api::request_access::{request_access, se_res_json, AccessResponse};
use tradovate_api::socket::{Socket};
use tradovate_api::user_sync::{UserSyncData};
use tradovate_api::api_client::{APIClient};

use std::{env,};

fn main() {
   
    //Set the const vars in enviroment
    env::set_var("HTTP_URL", "https://demo.tradovateapi.com/v1");
    env::set_var("WS_URL", "wss://demo.tradovateapi.com/v1/websocket");
    env::set_var("MD_URL", "wss://md.tradovateapi.com/v1/websocket");
    env::set_var("MD_DEMO_URL", "wss://md-demo.tradovateapi.com/v1/websocket");
    env::set_var("REPLAY_URL", "wss://replay.tradovateapi.com/v1/websocket");
    env::set_var("USER", "user_name");
    env::set_var("PASS", "account_pass");
    env::set_var("SEC", "your_api_key");
    env::set_var("CID", "your_CID");
    env::set_var("ACCESS_TOKEN", "Not Set");
    env::set_var("MD_ACCESS_TOKEN", "Not Set");
    env::set_var("TOKEN_EXPIRATION", "Not Set");
    ////////////////////////////////////////////////////////////////////////////////
    
    //get access credentials using reqwest over HTTPS
    let _res: AccessResponse = se_res_json(request_access().unwrap());
    //Connect a websocket client
    let mut  api_client = APIClient::new("MNQM2", 2553028);
    let mut _sync_data = UserSyncData::default();
    loop{
        api_client.check_msg(&mut _sync_data);
    }
    //NinjaCommand::limit_order("Sim101", "MNQ 06-22", "BUY", 1, 12456.00);
    //NinjaCommand::flatten_everything();

}
