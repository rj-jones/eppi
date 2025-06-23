use anyhow::Context;
use peppi::{game::immutable::Game, io::slippi::read};
use std::{
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    thread,
    time::{self, Duration},
};
