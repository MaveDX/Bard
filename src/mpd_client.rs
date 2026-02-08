use mpd::{Client, Song, Status};
use std::net::TcpStream;
use std::time::Duration;
use std::ops::RangeFull;
use anyhow::Result;

pub struct MPDClient {
    client: Client<TcpStream>,
}

impl MPDClient {
    pub fn new() -> Result<Self> {
        let client = Client::connect("127.0.0.1:6600")?;
        Ok(Self { client })
    }

    pub fn status(&mut self) -> Result<Status> {
        Ok(self.client.status()?)
    }

    pub fn current_song(&mut self) -> Result<Option<Song>> {
        Ok(self.client.currentsong()?)
    }

    pub fn play(&mut self) -> Result<()> {
        Ok(self.client.play()?)
    }

    pub fn play_pos(&mut self, pos: u32) -> Result<()> {
        Ok(self.client.switch(pos)?)
    }

    pub fn pause(&mut self, pause: bool) -> Result<()> {
        Ok(self.client.pause(pause)?)
    }

    pub fn stop(&mut self) -> Result<()> {
        Ok(self.client.stop()?)
    }

    pub fn next(&mut self) -> Result<()> {
        Ok(self.client.next()?)
    }

    pub fn previous(&mut self) -> Result<()> {
        Ok(self.client.prev()?)
    }

    pub fn seek(&mut self, time: Duration) -> Result<()> {
        // Seek within the currently playing song (get its queue position first)
        let status = self.client.status()?;
        if let Some(place) = status.song {
            Ok(self.client.seek(place.pos, time)?)
        } else {
            Ok(())
        }
    }

    pub fn set_volume(&mut self, volume: i8) -> Result<()> {
        Ok(self.client.volume(volume)?)
    }

    pub fn get_queue(&mut self) -> Result<Vec<Song>> {
        Ok(self.client.queue()?)
    }

    pub fn shuffle(&mut self) -> Result<()> {
        Ok(self.client.shuffle(RangeFull)?)
    }

    pub fn repeat(&mut self, repeat: bool) -> Result<()> {
        Ok(self.client.repeat(repeat)?)
    }

    pub fn random(&mut self, random: bool) -> Result<()> {
        Ok(self.client.random(random)?)
    }

    pub fn list_all(&mut self) -> Result<Vec<Song>> {
        Ok(self.client.listall()?)
    }

    pub fn clear(&mut self) -> Result<()> {
        Ok(self.client.clear()?)
    }

    pub fn update(&mut self) -> Result<()> {
        // Update MPD database - note: this is a fire-and-forget operation
        // We ignore the result as it's just for refreshing the database
        self.client.rescan().ok();
        Ok(())
    }

    pub fn add(&mut self, _uri: &str) -> Result<()> {
        // NOTE: This method is kept for API compatibility but is not used.
        // Songs are added via the mpc command-line tool in play_folder().
        // This works around the mpd crate's ToSongPath trait limitations.
        Ok(())
    }
}

pub fn format_time(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    format!("{}:{:02}", mins, secs)
}
