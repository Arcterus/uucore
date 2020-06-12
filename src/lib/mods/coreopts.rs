//! This module is designed to provide a wrapper over `clap` that mimicks the `getopts` crate's
//! API.  It automatically handles a couple common options as well (such as `--help` and
//! `--version`).  In the future, this module will simply provide helpers for `clap`, completely
//! shedding itself of the current `getopts`-like APIs (or that I at least what I hope will occur).

use clap::{App, Arg, ArgMatches};

use std::borrow::ToOwned;
use std::collections::HashMap;
use std::process;
use std::rc::Rc;
use std::str;

pub struct HelpText<'a> {
    name: &'a str,
    version: Option<&'a str>,
    syntax: Option<&'static str>,
    summary: Option<&'a str>,
    long_help: Option<&'a str>,
}

#[derive(Default)]
pub struct HelpTextBuilder<'a> {
    name: &'a str,
    version: Option<&'a str>,
    syntax: Option<String>,
    summary: Option<&'a str>,
    long_help: Option<&'a str>,
}

impl<'a> HelpTextBuilder<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,

            ..Default::default()
        }
    }

    pub fn version(mut self, version: &'a str) -> Self {
        self.version = Some(version);
        self
    }

    pub fn syntax(mut self, syntax: &'a str) -> Self {
        self.syntax = Some(format!("{} {}", self.name, syntax));
        self
    }

    pub fn summary(mut self, summary: &'a str) -> Self {
        self.summary = Some(summary);
        self
    }

    pub fn long_help(mut self, long_help: &'a str) -> Self {
        self.long_help = Some(long_help);
        self
    }

    // FIXME: leaks memory, but unsure how to fix otherwise as `clap` only allows Into<&str> for
    //        `App::usage()`.
    pub fn build(self) -> HelpText<'a> {
        HelpText {
            name: self.name,
            version: self.version,
            syntax: self.syntax.map(|val| &*Box::leak(val.into())),
            summary: self.summary,
            long_help: self.long_help,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Matches<'a> {
    pub free: Vec<String>,

    inner: ArgMatches<'a>,
    short_to_long: Rc<HashMap<&'a str, &'a str>>,
}

impl<'a> Matches<'a> {
    pub fn opt_present(&self, nm: &str) -> bool {
        let nm = self.convert_name(nm);
        self.inner.is_present(nm)
    }

    pub fn opts_present(&self, names: &[String]) -> bool {
        for nm in names {
            let nm = self.convert_name(nm);
            if self.inner.is_present(nm) {
                return true;
            }
        }

        false
    }

    pub fn opt_str(&self, nm: &str) -> Option<String> {
        let nm = self.convert_name(nm);
        self.inner.value_of(nm).map(ToOwned::to_owned)
    }

    fn convert_name<'b>(&self, nm: &'b str) -> &'b str
    where
        'a: 'b,
    {
        if nm.len() != 1 {
            nm
        } else {
            *self.short_to_long.get(&nm).unwrap_or(&nm)
        }
    }
}

pub struct CoreOptions<'a: 'b, 'b> {
    options: Option<App<'a, 'b>>,
    help_text: HelpText<'a>,
    short_to_long: Rc<HashMap<&'a str, &'a str>>,
}

impl<'a: 'b, 'b> CoreOptions<'a, 'b> {
    pub fn new(help_text: HelpText<'a>) -> Self {
        let mut app = App::new(help_text.name);
        if let Some(version) = help_text.version {
            app = app.version(version);
        }
        if let Some(syntax) = help_text.syntax {
            app = app.usage(syntax);
        }
        if let Some(summary) = help_text.summary {
            app = app.about(summary);
        }
        if let Some(long_help) = help_text.long_help {
            app = app.after_help(long_help);
        }
        app = app.arg(Arg::with_name("ARGS")
                      .index(1)
                      .multiple(true)
                      .hidden(true));

        CoreOptions {
            short_to_long: Rc::new(Default::default()),
            help_text,
            options: Some(app),
        }
    }

    // XXX: not sure if this is right
    // XXX: this does not allow hyphen values (at least for now) due to potential ambiguities
    pub fn optflagopt(
        &mut self,
        short_name: &'a str,
        long_name: &'a str,
        desc: &'a str,
        hint: &'a str,
    ) -> &mut CoreOptions<'a, 'b> {
        self.optcommon(short_name, long_name, desc, |arg| {
            arg.value_name(hint)
                .min_values(0)
                .allow_hyphen_values(false)
        })
    }

    pub fn optflag(
        &mut self,
        short_name: &'a str,
        long_name: &'a str,
        desc: &'a str,
    ) -> &mut CoreOptions<'a, 'b> {
        self.optcommon(short_name, long_name, desc, |arg| arg)
    }

    pub fn optflags(
        &mut self,
        short_names: &'a [&'a str],
        long_names: &'a [&'a str],
        desc: &'a str,
    ) -> &mut CoreOptions<'a, 'b> {
        let (short_names, short) = if short_names.is_empty() {
            (short_names, "")
        } else {
            (&short_names[1..], short_names[0])
        };

        let (long_names, long) = if long_names.is_empty() {
            (long_names, "")
        } else {
            (&long_names[1..], long_names[0])
        };

        self.optcommon(short, long, desc, |mut arg| {
            for &name in short_names {
                arg = arg.alias(name);
            }
            for &name in long_names {
                arg = arg.alias(name);
            }
            arg
        })
    }

    // XXX: this does not allow hyphen values (at least for now) due to ambiguities
    pub fn optflagmulti(
        &mut self,
        short_name: &'a str,
        long_name: &'a str,
        desc: &'a str,
    ) -> &mut CoreOptions<'a, 'b> {
        self.optcommon(short_name, long_name, desc, |arg| arg.multiple(true).allow_hyphen_values(false))
    }

    pub fn optopt(
        &mut self,
        short_name: &'a str,
        long_name: &'a str,
        desc: &'a str,
        hint: &'a str,
    ) -> &mut CoreOptions<'a, 'b> {
        self.optcommon(short_name, long_name, desc, |arg| arg.takes_value(true).value_name(hint))
    }

    pub fn optmulti(
        &mut self,
        short_name: &'a str,
        long_name: &'a str,
        desc: &'a str,
        hint: &'a str,
    ) -> &mut CoreOptions<'a, 'b> {
        self.optcommon(short_name, long_name, desc, |arg| arg.multiple(true).takes_value(true).value_name(hint))
    }

    fn optcommon<F>(&mut self, short_name: &'a str, long_name: &'a str, desc: &'a str, func: F) -> &mut CoreOptions<'a, 'b>
    where
        F: Fn(Arg<'a, 'b>) -> Arg<'a, 'b>,
    {
        let options = self.options.take();
        self.options = options.map(|opts| {
            let arg = if !long_name.is_empty() {
                let long = Arg::with_name(long_name)
                    .long(long_name);

                if !short_name.is_empty() {
                    Rc::get_mut(&mut self.short_to_long).unwrap().insert(short_name, long_name);

                    long.short(short_name)
                } else {
                    long
                }
            } else if !short_name.is_empty() {
                Arg::with_name(short_name)
                    .short(short_name)
            } else {
                // TODO: gracefully handle errors rather than panicking
                panic!("option has neither a short nor a long name")
            };

            opts.arg(func(arg.help(desc).allow_hyphen_values(true)))
        });

        self
    }

    pub fn parse(&mut self, args: Vec<String>) -> Matches<'a> {
        let matches = match self.options.clone().unwrap().get_matches_from_safe(&args[..]) {
            Ok(m) => m,
            Err(ref f) if f.kind == clap::ErrorKind::HelpDisplayed || f.kind == clap::ErrorKind::VersionDisplayed => {
                print!("{}", f);
                process::exit(0);
            }
            Err(f) => {
                eprintln!("{}: {}", self.help_text.name, f);
                process::exit(1);
            }
        };

        let free = matches.values_of("ARGS").map(|vals| vals.map(ToOwned::to_owned).collect()).unwrap_or_default();

        Matches {
            short_to_long: self.short_to_long.clone(),
            free,
            inner: matches,
        }
    }
}

#[macro_export]
macro_rules! app {
    ($syntax: expr, $summary: expr, $long_help: expr) => {{
        let __help_text = uucore::coreopts::HelpTextBuilder::new(executable!())
            .version(env!("CARGO_PKG_VERSION"))
            .syntax($syntax)
            .summary($summary)
            .long_help($long_help)
            .build();
        uucore::coreopts::CoreOptions::new(__help_text)
    }};
}
