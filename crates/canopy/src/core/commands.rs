use std::{collections::HashMap, marker::PhantomData};

use crate::{
    Context,
    core::{Core, NodeId, context::CoreContext},
    error::{Error, Result},
    state::NodeName,
};

/// Supported argument types for command signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgTypes {
    /// Dynamic context argument.
    Context,
    /// Signed integer argument.
    ISize,
}

/// Runtime command argument values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Args {
    /// Context placeholder.
    Context,
    /// Signed integer value.
    ISize(isize),
}

impl Args {
    /// Return the contained `isize` value.
    pub fn as_isize(&self) -> Result<isize> {
        match self {
            Self::ISize(i) => Ok(*i),
            _ => Err(Error::Internal(format!("Expected isize, got {self:?}"))),
        }
    }
}

/// The return type of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTypes {
    /// No return value.
    Void,
    /// String return value.
    String,
}

/// The return type of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReturnSpec {
    /// What is the ultimate type of the return?
    pub typ: ReturnTypes,
    /// Is the return wrapped in a `Result`? That is, is the method fallible?
    pub result: bool,
}

impl ReturnSpec {
    /// Construct a return specification.
    pub fn new(typ: ReturnTypes, result: bool) -> Self {
        Self { typ, result }
    }
}

/// Runtime return values from command dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnValue {
    /// No return value.
    Void,
    /// String return value.
    String(String),
}

/// A parsed command invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    /// The name of the node.
    pub node: NodeName,
    /// The name of the command.
    pub command: String,
    /// Arguments to the command.
    pub args: Vec<Args>,
}

/// CommandDefinition encapsulates the definition of a command that can be
/// performed on a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    /// The name of the node.
    pub node: NodeName,
    /// The name of the command.
    pub command: String,
    /// A doc string taken from the method comment.
    pub docs: String,
    /// The return type of the command.
    pub ret: ReturnSpec,
    /// Argument types for the command.
    pub args: Vec<ArgTypes>,
}

impl CommandSpec {
    /// A full command name, of the form nodename.command
    pub fn fullname(&self) -> String {
        format!("{}.{}", self.node, self.command)
    }
}

/// Typed reference to a command defined on a widget type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRef<T> {
    /// Node name associated with the command.
    node: NodeName,
    /// Command identifier.
    command: &'static str,
    /// Argument types defined by the command signature.
    arg_types: &'static [ArgTypes],
    /// Marker for the command's widget type.
    _marker: PhantomData<fn() -> T>,
}

impl<T> CommandRef<T> {
    /// Create a typed command reference.
    pub fn new(node: NodeName, command: &'static str, arg_types: &'static [ArgTypes]) -> Self {
        Self {
            node,
            command,
            arg_types,
            _marker: PhantomData,
        }
    }

    /// Build a call to this command with no additional arguments.
    pub fn call(self) -> CommandCall<T> {
        CommandCall {
            command: self,
            args: Vec::new(),
        }
    }

    /// Build a call to this command with isize arguments.
    pub fn call_with<I>(self, args: I) -> CommandCall<T>
    where
        I: IntoIterator<Item = isize>,
    {
        CommandCall {
            command: self,
            args: args.into_iter().collect(),
        }
    }

    /// Return the number of non-context arguments expected by this command.
    fn expected_args(&self) -> usize {
        expected_isize_args(self.arg_types)
    }
}

/// A typed command invocation with arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandCall<T> {
    /// Target command reference.
    command: CommandRef<T>,
    /// Arguments provided to the command.
    args: Vec<isize>,
}

impl<T> CommandCall<T> {
    /// Build a command invocation for direct dispatch.
    pub fn invocation(self) -> CommandInvocation {
        let mut args = Vec::new();
        let mut arg_types = self.command.arg_types;
        if !arg_types.is_empty() && arg_types[0] == ArgTypes::Context {
            args.push(Args::Context);
            arg_types = &arg_types[1..];
        }
        debug_assert_eq!(arg_types.len(), self.args.len());
        for arg in self.args {
            args.push(Args::ISize(arg));
        }
        CommandInvocation {
            node: self.command.node,
            command: self.command.command.to_string(),
            args,
        }
    }
}

/// Render a command binding into a script string.
pub trait CommandBinding {
    /// Build the script source for this command binding.
    fn script(self) -> Result<String>;
}

impl<T> CommandBinding for CommandRef<T> {
    fn script(self) -> Result<String> {
        let expected = self.expected_args();
        if expected != 0 {
            return Err(Error::Invalid(format!(
                "command {}::{} expects {expected} arguments",
                self.node, self.command
            )));
        }
        Ok(format_command_call(&self.node, self.command, &[]))
    }
}

impl<T> CommandBinding for CommandCall<T> {
    fn script(self) -> Result<String> {
        let expected = self.command.expected_args();
        if expected != self.args.len() {
            return Err(Error::Invalid(format!(
                "command {}::{} expects {expected} arguments, got {}",
                self.command.node,
                self.command.command,
                self.args.len()
            )));
        }
        Ok(format_command_call(
            &self.command.node,
            self.command.command,
            &self.args,
        ))
    }
}

/// The CommandNode trait is implemented by widgets to expose commands.
pub trait CommandNode {
    /// Return a list of commands for this node.
    fn commands() -> Vec<CommandSpec>
    where
        Self: Sized;

    /// Dispatch a command to this node.
    fn dispatch(&mut self, c: &mut dyn Context, cmd: &CommandInvocation) -> Result<ReturnValue>;
}

/// Dispatch a command relative to a node.
pub fn dispatch(
    core: &mut Core,
    current_id: NodeId,
    cmd: &CommandInvocation,
) -> Result<Option<ReturnValue>> {
    if let Some(ret) = dispatch_subtree(core, current_id, cmd)? {
        return Ok(Some(ret));
    }

    let mut current = core.nodes[current_id].parent;
    while let Some(id) = current {
        if let Some(ret) = dispatch_on_node(core, id, cmd)? {
            return Ok(Some(ret));
        }
        current = core.nodes[id].parent;
    }

    Ok(None)
}

/// Count the number of isize arguments in a command signature.
fn expected_isize_args(arg_types: &[ArgTypes]) -> usize {
    arg_types
        .iter()
        .filter(|arg| matches!(arg, ArgTypes::ISize))
        .count()
}

/// Render a command call string for a node and argument list.
fn format_command_call(node: &NodeName, command: &str, args: &[isize]) -> String {
    if args.is_empty() {
        return format!("{node}::{command}()");
    }
    let rendered = args
        .iter()
        .map(|arg| arg.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{node}::{command}({rendered})")
}

/// Dispatch a command within the subtree rooted at `root`.
fn dispatch_subtree(
    core: &mut Core,
    root: NodeId,
    cmd: &CommandInvocation,
) -> Result<Option<ReturnValue>> {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(ret) = dispatch_on_node(core, id, cmd)? {
            return Ok(Some(ret));
        }
        let children = core.nodes[id].children.clone();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    Ok(None)
}

/// Dispatch a command on a node if its name matches the invocation.
fn dispatch_on_node(
    core: &mut Core,
    node_id: NodeId,
    cmd: &CommandInvocation,
) -> Result<Option<ReturnValue>> {
    let name_match = core.nodes[node_id].name == cmd.node;
    if !name_match {
        return Ok(None);
    }

    let result = core.with_widget_mut(node_id, |widget, core| {
        let mut ctx = CoreContext::new(core, node_id);
        widget.dispatch(&mut ctx, cmd)
    });

    match result {
        Ok(ret) => Ok(Some(ret)),
        Err(Error::UnknownCommand(_)) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Collection of available commands keyed by name.
#[derive(Debug)]
pub struct CommandSet {
    /// Command lookup table by full name.
    commands: HashMap<String, CommandSpec>,
}

impl Default for CommandSet {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandSet {
    /// Construct an empty command set.
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Add command specs to the set.
    pub fn add(&mut self, cmds: &[CommandSpec]) {
        for i in cmds {
            self.commands.insert(i.fullname(), i.clone());
        }
    }

    /// Get a command by fully qualified name.
    pub fn get(&self, name: &str) -> Option<&CommandSpec> {
        self.commands.get(name)
    }

    /// Return an iterator over all command specs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CommandSpec)> {
        self.commands.iter()
    }
}
