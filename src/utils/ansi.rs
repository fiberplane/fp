use std::{
    collections::VecDeque,
    io::{self, Read},
};

use vte::{Params, Parser, Perform};

pub enum Action {
    Text(String),
    Color(), //todo
    DeleteChar(usize),
}

pub struct Collector {
    actions: VecDeque<Action>,
}

impl Collector {
    pub fn new() -> Self {
        Self {
            actions: Default::default(),
        }
    }

    fn append_char(&mut self, c: char) {
        if let Some(Action::Text(s)) = self.actions.back_mut() {
            s.push(c);
            return;
        }

        self.actions.push_back(Action::Text(String::from(c)));
    }
}

impl Perform for Collector {
    fn print(&mut self, c: char) {
        self.append_char(c)
    }

    fn execute(&mut self, byte: u8) {
        println!("[execute] {:02x}", byte);
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
        println!(
            "[hook] params={:?}, intermediates={:?}, ignore={:?}, char={:?}",
            params, intermediates, ignore, c
        );
    }

    fn put(&mut self, byte: u8) {
        println!("[put] {:02x}", byte);
    }

    fn unhook(&mut self) {
        println!("[unhook]");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        println!(
            "[osc_dispatch] params={:?} bell_terminated={}",
            params, bell_terminated
        );
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
        println!(
            "[csi_dispatch] params={:#?}, intermediates={:?}, ignore={:?}, char={:?}",
            params, intermediates, ignore, c
        );
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        println!(
            "[esc_dispatch] intermediates={:?}, ignore={:?}, byte={:02x}",
            intermediates, ignore, byte
        );
    }
}
