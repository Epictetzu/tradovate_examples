//My libraries for accessing Tradovate's API endopoints
use tradovate_api::request_access::{request_access, se_res_json, AccessResponse};
use tradovate_api::socket::{Socket}; 
use tradovate_api::md_socket::{MDSocket, PertinentMarketData, SavedMarketData, MDClient};
use tradovate_api::orders::{BracketOrders};
use tradovate_api::user_sync::{UserSyncData};
use tradovate_api::api_client::{APIClient, PertinentUserData, StrategyDetails};
use tradovate_api::StrategyState;
use tradovate_api::dates_and_times::{is_market_open};
//Standard libraries
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::{thread, env, time};
//Multithreading libraries
use crossbeam::channel::{unbounded, Receiver, Sender};
use crossbeam::utils::{Backoff};
use crossbeam::sync::Parker;
//use chrono::prelude::*;




fn main() {
    if is_market_open() {
        println!("Market is open");
    } else {
        println!("Market is closed");
        return;
    }
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
    //Create and connect an api client struct for API Access 
    let api_access = Arc::new(Mutex::new(APIClient::new("MNQM2", 2553028)));//Use this outside spawned thread, but prefer to use channel messages instead.
    let api_access_loop_clone = Arc::clone(&api_access);//Use this inside of spawned thread
    //Create and connect a md client struct for Market Data Access
    let mut md_access = MDClient::new("MNQM2", 2553028);
    //
    //Create the UserSyncData and Market Datastruct to hold the user's synced information and market data
    //don't call these call the arc mutexes
    let mut _sync_data = UserSyncData::default();
    let mut _md_data = SavedMarketData::default();
     
    //Convert the data to Arc<Mutex> call these from outside the scope of the spawned threads
    let saved_user_data = Arc::new(Mutex::new(_sync_data));
    let saved_market_data = Arc::new(Mutex::new(_md_data));

    //Clone Data safes for arc mutex data structures use inside of spawned threads
    let user_data_loop_clone = Arc::clone(&saved_user_data);
    let market_data_loop_clone = Arc::clone(&saved_market_data);
    ////////////////////////////////////////////////////////////////////////////////
    
    
    ////////////////////////////////////////////////////////////////////////////////
    //Aunthenicate the api client and do a user sync to get the user's data and subscribe to user updates
    api_access_loop_clone.lock().unwrap().check_msg(&mut user_data_loop_clone.lock().unwrap());//Authenicate by checking msg at least once
    ////////////////////////////////////////////////////////////////////////////////
    
    //These are for parking the main thread while it waits for new data from the sockets
    let bot_parker = Parker::new();
    let bot_parker_clone = bot_parker.unparker().clone();
    let bot_parker_clone2 = bot_parker.unparker().clone();
    //
    
    
    //Thread for the Market Data loop
    //Save pertinent marktet data in a Rwlock so it can be eaisly check and updated from multiple threads
    //The api access thread should be the only thread that will be writing to this data
    let pertinent_market_data = Arc::new(RwLock::new(PertinentMarketData::default()));
    let pertinent_market_data_arc = pertinent_market_data.clone();

    //Use this bool to stop the thread to exit the application
    let md_loop_breaker = Arc::new(AtomicBool::new(true));//Loop breaker is used to break the loop later
    let md_loop_breaker0 = Arc::clone(&md_loop_breaker); //Loop breaker must be a cloned arc mutex reference this one goes inside the thread
    
    //This atomicbool is for comunicatinig to the robot there is new data to act upon.
    let new_msg_from_md = Arc::new(AtomicBool::new(false));
    let new_msg_from_md_arc = Arc::clone(&new_msg_from_md);

    let _md_thread = std::thread::spawn(move || { 
        
        md_access.md_check_msg(&mut market_data_loop_clone.lock().unwrap());//Authenicate before anything else by checking msg at least once
        md_access.subscribe_chart("MNQM2", 2553028); //Subscribe to the chart
        
        ////////////////////////////////////////////////////////////////////////////////
        while md_loop_breaker0.load(Ordering::Relaxed) {
            md_access.md_check_msg(&mut market_data_loop_clone.lock().unwrap());
            
            let new_data = md_access.update_pertinent_market_data(&mut market_data_loop_clone.lock().unwrap());//Update the market data
            if *pertinent_market_data_arc.read().unwrap() != new_data { //If the new data is different than the old data
                println!("{:?}", new_data);
                new_msg_from_md_arc.store(true, Ordering::Relaxed);//Set the atomic bool to true
                *pertinent_market_data_arc.write().unwrap() = new_data; //Set the old data var to the new data to use next loop
                bot_parker_clone.unpark();//Unpark the robot thread
            }
            thread::sleep(time::Duration::from_millis(100));
            md_access.heartbeat_check();
        }
        //    
        ////////////////////////////////////////////////////////////////////////////////
        //Call these functios before exiting the thread
        md_access.unsubscribe_all();
        md_access.disconnect_md_socket();
    });
    /////
    //
    
    
    //Thread for the API Access loop
    //
    //Use this bool to stop the thread
    let api_loop_breaker = Arc::new(AtomicBool::new(true));//Loop breaker is used to break the loop later
    let api_loop_breaker0 = Arc::clone(&api_loop_breaker); //Loop breaker must be a cloned arc mutex reference this one goes inside the thread
    //This atomicbool is for comunicatinig to the robot there is new data to act upon.
    let new_msg_from_api = Arc::new(AtomicBool::new(false));
    let new_msg_from_api_arc = Arc::clone(&new_msg_from_api);

    //Save pertinent user data in a Rwlock so it can be eaisly check and updated from multiple threads
    //The api access thread should be the only thread that will be writing to this data
    let pertinent_user_data = Arc::new(RwLock::new(PertinentUserData::default()));
    let pertinent_user_data_arc = pertinent_user_data.clone();
    
    //Create the channel for the robot to send to the api_client loop
    let (robot_sender_to_api, api_receiver_from_robot): (Sender<(&str, StrategyDetails)>, Receiver<(&str, StrategyDetails)>) = unbounded();
    //Spawn the thread
    let _api_thread = std::thread::spawn(move || { 
        
        //While loop breaker is true
        while api_loop_breaker0.load(Ordering::Acquire) {
            thread::sleep(time::Duration::from_millis(10));
            
                 
            api_access_loop_clone.lock().unwrap().check_msg(&mut user_data_loop_clone.lock().unwrap());
            
            let new_data: PertinentUserData =  api_access_loop_clone.lock().unwrap().update_pertinent_user_data(&mut user_data_loop_clone.lock().unwrap());    
            //Check if the new data is different than the old data using read lock so it can still be read from other threads as we may not need to write to it
            //If the new data is different than the old data then we will use the write lock to update the pertinent user data for all threads to see
            if *pertinent_user_data_arc.read().unwrap() != new_data { 
                //println!("{:?}", new_data);
                new_msg_from_api_arc.store(true, Ordering::Release);   
                *pertinent_user_data_arc.write().unwrap() = new_data;
                bot_parker_clone2.unpark();//Unpark the robot thread
            }
            if !api_receiver_from_robot.is_empty() {//If there is a message from the robot
                let (action, details) = api_receiver_from_robot.recv().unwrap();
                println!("{}ing", action);
                match action {
                    "Buy"|"buy" => {api_access_loop_clone.lock().unwrap().strategy_long_entry(&mut user_data_loop_clone.lock().unwrap(), details);},
                    "Sell"|"sell" => {api_access_loop_clone.lock().unwrap().strategy_short_entry(&mut user_data_loop_clone.lock().unwrap(), details);},
                    _ => {println!("Unknown action: {}", action);}
                }
            }            
            api_access_loop_clone.lock().unwrap().heartbeat_check();    
            api_access_loop_clone.lock().unwrap().reset_timer(); //Reset the the orders every 15 minutes
        }
        //Call these functios before exiting the thread
        api_access.lock().unwrap().cancel_and_exit(761574, 2553028, false); 
        api_access_loop_clone.lock().unwrap().disconnect_socket();
    });
    ////////////////////////////////////////////////////////////////////////////////
        
    
    
    //Thread for the robot loop
    ////////////////////////////////////////////////////////////////////////////////
    thread::sleep(time::Duration::from_millis(5000));
    
    // Create the robot robot for controlling the strategy
    let mut robot = Robot::new();
    robot.strategy_details.strategy_state = StrategyState::StartUp;
    //Create the channel for the robot to send to the api access

    loop {
        //Setup a crossbeam backoff timer to limit how much this just spins 
        let backoff = Backoff::new();
        //While there are no new messages just do backoff snooze
        while !new_msg_from_md.load(Ordering::Relaxed) && !new_msg_from_api.load(Ordering::Relaxed) {
            if backoff.is_completed() {
                //println!("Parked loop");
                bot_parker.park();
            }
            robot.state_machine(&robot_sender_to_api);
            //println!("Snooze loop");
            backoff.snooze();
        }
        
        if new_msg_from_api.load(Ordering::Relaxed) == true {
            //println!("New message from API");
            robot.strategy_details.pertinent_user_data = pertinent_user_data.read().unwrap().clone();
            new_msg_from_api.store(false, Ordering::Relaxed);
        }
        if new_msg_from_md.load(Ordering::Relaxed) == true {
            //println!("New message from MD");
            robot.strategy_details.pertinent_market_data = pertinent_market_data.read().unwrap().clone();
            
            new_msg_from_md.store(false, Ordering::Relaxed);
        }
        
        //Run the robot's state machine one time through
        robot.state_machine(&robot_sender_to_api);
        
        
    } 
    //
    ////////////////////////////////////////////////////////////////////////////////
    
    
    /* //API client Thread breaker
    api_loop_breaker.store(false, Ordering::Release);//This breaks the api access thread
    //Market Data client Thread breaker
    md_loop_breaker.store(false, Ordering::Release);//This breaks the api access thread
    //Join the threads back to the main thread
    _api_thread.join().unwrap();
    _md_thread.join().unwrap(); */
    
}
    
    
    



struct Robot {
    pub strategy_details: StrategyDetails,
    //pub trade_timer: DateTime<Local>,
}
impl Robot {
    pub fn new() -> Robot {
        Robot {
            //trade_timer: Local::now(),
            strategy_details: StrategyDetails {
                symbol: "MNQM2".to_string(),
                contract_id: 2553028,
                order_type: "Stop".to_string(),
                stop_multiplier: 0.5,
                tp_multiplier: 0.1,
                max_stop_loss: 5.0,
                pertinent_user_data: PertinentUserData::default(),
                pertinent_market_data: PertinentMarketData::default(),
                strategy_state: StrategyState::default(),
            },
        }
    }
    fn state_machine(&mut self, sender_to_api: &Sender<(&str, StrategyDetails)>) {
        thread::sleep(time::Duration::from_millis(10));
            
        match self.strategy_details.strategy_state {
            StrategyState::Idle => {},
            StrategyState::StartUp => { 
                thread::sleep(time::Duration::from_millis(10));
                println!("StartUp");
                //Startup stuff goes here
                println!("Progress to ready");
                self.strategy_details.strategy_state = StrategyState::Ready; 
            },  
            StrategyState::Ready => { 
                thread::sleep(time::Duration::from_millis(10));
                
                if self.strategy_details.pertinent_user_data.in_position == false{
                    if self.strategy_details.pertinent_user_data.active_buy_order_id == 0 {
                        println!("Pew");
                        sender_to_api.send(("Buy", self.strategy_details.clone())).unwrap();
                    }
                    if self.strategy_details.pertinent_user_data.active_sell_order_id == 0 {
                        println!("Pew Pew");
                        sender_to_api.send(("Sell", self.strategy_details.clone())).unwrap();
                    }
                    thread::sleep(time::Duration::from_millis(5000));
                    println!("Transitioning to WaitingForEntry");
                    self.strategy_details.strategy_state = StrategyState::WaitingForEntry;
                    return;
                }else if self.strategy_details.pertinent_user_data.in_position == true {
                    println!("InTrade");
                    self.strategy_details.strategy_state = StrategyState::InTrade;
                    return;
                }
                
            },
            StrategyState::WaitingForEntry => {
                
                if self.strategy_details.pertinent_user_data.in_position == false {  //If we are not in position
                    if self.strategy_details.pertinent_user_data.active_buy_order_id == 0  //And we have no active buy order
                    || self.strategy_details.pertinent_user_data.active_sell_order_id == 0 { //Or we have no active sell order
                        self.strategy_details.strategy_state = StrategyState::Ready;
                        println!("Transitioning to Ready");
                        return;
                        //TODO: Update orders and modify if needed
                    }
                    else if self.strategy_details.pertinent_user_data.active_buy_order_id != 0  //if we are not in positionAnd we have an active buy order
                    && self.strategy_details.pertinent_user_data.active_sell_order_id != 0 { //And we have an active sell order
                        //thread::sleep(time::Duration::from_millis(500));
                        return;
                    }
                }      
                else if self.strategy_details.pertinent_user_data.in_position == true {
                    println!("Transitioning to Intrade");
                    self.strategy_details.strategy_state = StrategyState::InTrade;
                    return;
                }
                thread::sleep(time::Duration::from_millis(10));
            },        
            StrategyState::InTrade => {
                

                if self.strategy_details.pertinent_user_data.in_position == false {
                    println!("Transitioning to Ready");
                    self.strategy_details.strategy_state = StrategyState::Ready;
                    thread::sleep(time::Duration::from_millis(450000));
                }
                thread::sleep(time::Duration::from_millis(5000));
                
            },
            StrategyState::Shutdown => { /* cancel and exit */ },
        }
    }
                
            
            
    /*  */
    /*  */
    
    
    


}

 
  
     
