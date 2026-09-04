use std::{sync::mpsc::{self, Sender}, thread::self, time::{Duration, Instant}};

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};

pub enum Event {
    Tick,
    Key(KeyEvent),
    Mouse(MouseEvent)
}

#[derive(Debug)]
pub struct EventThread {
    #[allow(unused)]
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
}

impl EventThread {
    pub fn new(tick_rate: u64) -> Self {
        let (sender, receiver) = mpsc::channel();
        let tick_rate = Duration::from_millis(tick_rate);
        let _ = {
            let sender: Sender<Event> = sender.clone();
            thread::spawn(move || {
                let mut last_tick = Instant::now();
                loop {
                    let timeout = tick_rate
                                        .checked_sub(last_tick.elapsed())
                                        .unwrap_or(tick_rate);
                    if event::poll(timeout).expect("poll error") {
                        match event::read().expect("read error") {
                            CrosstermEvent::Key(e) => {
                                if e.kind == event::KeyEventKind::Press {
                                    sender.send(Event::Key(e))
                                } else {
                                    Ok(())
                                }
                            },
                            CrosstermEvent::Mouse(e) => sender.send(Event::Mouse(e)),
                            _ => Ok(()),
                        }
                        .expect("event match error")
                    }

                    if last_tick.elapsed() >= tick_rate {
                        sender.send(Event::Tick).expect("send error");
                        last_tick = Instant::now()
                    }
                }
            })
        };

        Self {
            sender,
            receiver,
        }
    }

    pub fn read(&self) -> Result<Event, Box<dyn std::error::Error>> {
        Ok(self.receiver.recv()?)
    }
}

