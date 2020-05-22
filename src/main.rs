#![recursion_limit = "128"]

mod channel;
mod clipboard;
mod controller;
mod edit_view;
mod linecache;
mod main_win;
mod prefs_win;
mod proto;
mod rpc;
mod scrollable_drawing_area;
mod theme;
mod xi_thread;

use crate::channel::Sender;
use crate::controller::{Controller, CoreMsg};
use dirs_next::home_dir;
use gio::prelude::*;
use gio::{ApplicationExt, ApplicationFlags, FileExt};
use glib::clone;
use gtk::Application;
use log::*;
use main_win::MainWin;
use rpc::{Core, Handler};
use serde_json::Value;
use std::any::Any;
use std::cell::RefCell;
use std::env::args;

trait IdleCallback: Send {
    fn call(self: Box<Self>, a: &Any);
}

impl<F: FnOnce(&Any) + Send> IdleCallback for F {
    fn call(self: Box<F>, a: &Any) {
        (*self)(a)
    }
}

#[derive(Clone)]
struct MyHandler {
    sender: Sender<CoreMsg>,
}

impl MyHandler {
    fn new(sender: Sender<CoreMsg>) -> MyHandler {
        MyHandler { sender }
    }
}

impl Handler for MyHandler {
    fn notification(&self, method: &str, params: &Value) {
        debug!(
            "CORE --> {{\"method\": \"{}\", \"params\":{}}}",
            method, params
        );
        let method2 = method.to_string();
        let params2 = params.clone();
        self.sender.send(CoreMsg::Notification {
            method: method2,
            params: params2,
        });
    }
}

fn main() {
    env_logger::init();

    let controller = Controller::new();
    let controller2 = controller.clone();
    let (chan, sender) = channel::Channel::new(move |msg| {
        controller2.borrow().handle_msg(msg);
    });
    controller.borrow_mut().set_sender(sender.clone());
    controller.borrow_mut().set_channel(chan);

    let (xi_peer, rx) = xi_thread::start_xi_thread();
    let handler = MyHandler::new(sender.clone());
    let core = Core::new(xi_peer, rx, handler.clone());
    controller.borrow_mut().set_core(core);

    let application =
        Application::new(Some("com.github.bvinc.gxi"), ApplicationFlags::HANDLES_OPEN)
            .expect("failed to create gtk application");

    let mut config_dir = None;
    let mut plugin_dir = None;
    if let Some(home_dir) = home_dir() {
        let xi_config = home_dir.join(".config").join("xi");
        let xi_plugin = xi_config.join("plugins");
        config_dir = xi_config.to_str().map(|s| s.to_string());
        plugin_dir = xi_plugin.to_str().map(|s| s.to_string());
    }

    application.connect_startup(clone!(@strong controller => move |application| {
        debug!("startup");
        controller.borrow().core().client_started(config_dir.clone(), plugin_dir.clone());

        let main_win = MainWin::new(application, controller.clone());
        controller.borrow_mut().set_main_win(main_win);
    }));

    application.connect_activate(clone!(@strong controller => move |application| {
        debug!("activate");

        controller.borrow().req_new_view(None);
    }));

    application.connect_open(clone!(@strong controller => move |_,files,s| {
        debug!("open");

        for file in files {
            let path = file.get_path();
            if path.is_none() { continue; }
            let path = path.unwrap();
            let path = path.to_string_lossy().into_owned();

            controller.borrow().req_new_view(Some(&path));
        }
    }));
    application.connect_shutdown(move |_| {
        debug!("shutdown");
    });

    application.run(&args().collect::<Vec<_>>());
}
