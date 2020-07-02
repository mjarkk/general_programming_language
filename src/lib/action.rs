use super::*;

#[derive(Debug)]
pub enum Action {
  Variable(ActionVariable),
  Return(Option<Box<Action>>),
  Assigment(ActionAssigment),
  FunctionCall(ActionFunctionCall),
  VarRef(String),
}

#[derive(Debug, Clone)]
enum ActionVariableType {
  Const, // inmutatable
  Let,   // mutatable
}

#[derive(Debug)]
pub struct ActionVariable {
  name: String,
  var_type: ActionVariableType,
  type_: Option<Type>,
  action: Box<Action>,
}

impl Into<Action> for ActionVariable {
  fn into(self) -> Action {
    Action::Variable(self)
  }
}

#[derive(Debug)]
pub struct ActionAssigment {
  pub name: String,
  pub action: Box<Action>,
}

impl Into<Action> for ActionAssigment {
  fn into(self) -> Action {
    Action::Assigment(self)
  }
}

#[derive(Debug)]
pub struct ActionFunctionCall {
  pub name: String,
  pub arguments: Vec<Action>,
}

impl Into<Action> for ActionFunctionCall {
  fn into(self) -> Action {
    Action::FunctionCall(self)
  }
}

pub struct ParseAction<'a> {
  p: &'a mut Parser,
  res: Option<Action>,
  action_to_expect: ActionToExpect,
}

enum ParseActionState {
  Var(ParseActionStateVar),                   // let foo = bar
  Return(ParseActionStateReturn),             // return foo
  Assigment(ParseActionStateAssigment),       // foo = bar
  FunctionCall(ParseActionStateFunctionCall), // foo(bar)
  VarRef(String),                             // foo
}

struct ParseActionStateFunctionCall {
  name: String,
  arguments: Vec<Action>,
}

impl Into<ParseActionState> for ParseActionStateFunctionCall {
  fn into(self) -> ParseActionState {
    ParseActionState::FunctionCall(self)
  }
}

struct ParseActionStateAssigment {
  name: String,
  action: Option<Action>,
}

impl Into<ParseActionState> for ParseActionStateAssigment {
  fn into(self) -> ParseActionState {
    ParseActionState::Assigment(self)
  }
}

struct ParseActionStateReturn {
  action: Option<Action>, // The value to return
}

impl Into<ParseActionState> for ParseActionStateReturn {
  fn into(self) -> ParseActionState {
    ParseActionState::Return(self)
  }
}

struct ParseActionStateVar {
  var_type: ActionVariableType, // What kind of variable is this a const or a let
  name: Vec<u8>,                // The variable name
  type_: Option<Type>,          // The variable type
  action: Option<Action>,       // The value set to the variable
}

impl Into<ParseActionState> for ParseActionStateVar {
  fn into(self) -> ParseActionState {
    ParseActionState::Var(self)
  }
}

#[derive(PartialEq)]
pub enum ActionToExpect {
  ActionInBody, // A line in a function body
  Assignment, // A assingment of some sort, like the contents of a variable or a function argument or the value of the return
}

enum DetectedAction {
  VarRefName, // 1. Just a variable name `foo`
  Assignment, // 2. `foo = bar`
  Function,   // 3. functions `foo()`
              // 4. inline strings `"foo"`
              // 5. inline numbers `1`
              // 6. inline arrays `[foo, bar]`
              // 7. inline structs `foo{bar: baz}`
}

impl<'a> ParseAction<'a> {
  pub fn start(
    p: &'a mut Parser,
    go_back_one: bool,
    action_to_expect: ActionToExpect,
  ) -> Result<Action, ParsingError> {
    if go_back_one {
      p.index -= 1;
    }
    let mut s = Self {
      action_to_expect,
      p,
      res: None,
    };
    s.detect()?;
    if let Some(res) = s.res {
      Ok(res)
    } else {
      s.p.error(ParsingErrorType::UnexpectedResult)
    }
  }
  fn commit_state(&mut self, state: impl Into<ParseActionState>) -> Result<(), ParsingError> {
    self.res = Some(match state.into() {
      ParseActionState::Var(meta) => {
        if let None = meta.action {
          return self
            .p
            .error(ParsingErrorType::Custom("Missing variable assignment"));
        }

        ActionVariable {
          name: String::from_utf8(meta.name.clone()).unwrap(),
          var_type: meta.var_type.clone(),
          type_: meta.type_,
          action: Box::new(meta.action.unwrap()),
        }
        .into()
      }
      ParseActionState::Return(meta) => {
        let mut return_action: Option<Box<Action>> = None;
        if let Some(action) = meta.action {
          return_action = Some(Box::new(action));
        }
        Action::Return(return_action)
      }
      ParseActionState::Assigment(meta) => {
        if let None = meta.action {
          return self
            .p
            .error(ParsingErrorType::Custom("Missing variable assignment"));
        }

        ActionAssigment {
          name: meta.name,
          action: Box::new(meta.action.unwrap()),
        }
        .into()
      }
      ParseActionState::FunctionCall(meta) => ActionFunctionCall {
        name: meta.name,
        arguments: meta.arguments,
      }
      .into(),
      ParseActionState::VarRef(name) => Action::VarRef(name),
    });
    Ok(())
  }

  fn detect(&mut self) -> Result<(), ParsingError> {
    let matched_res = if self.action_to_expect == ActionToExpect::ActionInBody {
      self.p.try_match(&[
        (Keywords::Const, " \t\n"),
        (Keywords::Let, " \t\n"),
        (Keywords::Return, "} \t\n"),
      ])
    } else {
      // Matching keywords is only allowed when inside the body
      None
    };

    // Try to match a keyword and react to it
    if let Some(matched) = matched_res {
      match matched {
        Keywords::Const | Keywords::Let => {
          // Go to parsing the variable
          let var_type = if let Keywords::Const = matched {
            ActionVariableType::Const
          } else {
            ActionVariableType::Let
          };
          let new_var = self.parse_variable(Some(var_type))?;
          self.commit_state(new_var)?;
        }
        Keywords::Return => {
          // Go to parsing the return
          let to_commit = self.parse_return()?;
          self.commit_state(to_commit)?;
        }
      }
      return Ok(());
    }

    // We are in a wired state right now where a lot of things are possible like
    // 1. variable assgiment `foo` or `foo = bar` (the second one is not allowed when ActionToExpect is ActionInBody)
    // 2. functions `foo()`
    // 3. inline strings `"foo"`
    // 4. inline numbers `1`
    // 5. inline arrays `[foo, bar]`
    // 6. inline structs `foo{bar: baz}`
    //
    // The code underhere will detect what the action is,
    // TODO: 2, 3, 4, 5, 6
    let mut name: Vec<u8> = vec![];
    let mut detected_action = DetectedAction::VarRefName;
    let mut next_char = self.p.next_char();
    let mut name_completed = false;

    while let Some(c) = next_char {
      match c {
        ' ' | '\t' | '\n' => {
          if name.len() > 0 {
            name_completed = true;
          }
          // Else ignore this
        }
        _ if legal_name_char(c) && !name_completed => name.push(c as u8),
        '(' => {
          detected_action = DetectedAction::Function;
          break;
        }
        '}' if name_completed => {
          self.p.index -= 1;
          break;
        }
        '=' => {
          detected_action = DetectedAction::Assignment;
          break;
        }
        c => return self.p.unexpected_char(c),
      }
      next_char = self.p.next_char();
    }
    if let None = next_char {
      return self.p.unexpected_eof();
    }
    let name_string = String::from_utf8(name).unwrap();

    // Do things relative to the detected action
    match detected_action {
      DetectedAction::VarRefName => {
        self.commit_state(ParseActionState::VarRef(name_string))?;
      }
      DetectedAction::Assignment => {
        let res = self.parse_assignment(name_string, true)?;
        self.commit_state(res)?;
      }
      DetectedAction::Function => {
        let res = self.parse_function(name_string, false)?;
        self.commit_state(res)?;
      }
    };
    return Ok(());
  }
  fn parse_function(
    &mut self,
    name: String,
    check_for_function_open_sign: bool,
  ) -> Result<ParseActionStateFunctionCall, ParsingError> {
    let res = ParseActionStateFunctionCall {
      name,
      arguments: vec![],
    };

    if check_for_function_open_sign {
      match self.p.next_char() {
        Some('(') => {} // This is what we exect. return no error
        Some(c) => return self.p.unexpected_char(c),
        None => return self.p.unexpected_eof(),
      }
    }

    loop {
      // TODO: Add code for parsing the arguments
      break;
    }

    match self.p.next_while(" \t\n") {
      Some(')') => {} // This is what we exect. return no error
      Some(c) => return self.p.unexpected_char(c),
      None => return self.p.unexpected_eof(),
    }

    Ok(res)
  }
  fn parse_assignment(
    &mut self,
    name: String,
    check_for_equal_sign: bool,
  ) -> Result<ParseActionStateAssigment, ParsingError> {
    let mut res = ParseActionStateAssigment { name, action: None };

    if check_for_equal_sign {
      match self.p.next_while(" \t\n") {
        Some('=') => {}
        Some(c) => return self.p.unexpected_char(c),
        None => return self.p.unexpected_eof(),
      }
    }

    match self.p.next_while(" \t\n") {
      Some(_) => {
        let action = ParseAction::start(self.p, true, ActionToExpect::Assignment)?;
        res.action = Some(action);
      }
      None => return self.p.unexpected_eof(),
    }

    Ok(res)
  }
  fn parse_return(&mut self) -> Result<ParseActionStateReturn, ParsingError> {
    let mut res = ParseActionStateReturn { action: None };

    match self.p.next_while(" \t\n") {
      Some('}') => {}
      Some(_) => {
        let action = ParseAction::start(self.p, true, ActionToExpect::Assignment)?;
        res.action = Some(action);
      }
      None => return self.p.unexpected_eof(),
    }
    Ok(res)
  }
  fn parse_variable(
    &mut self,
    var_type_option: Option<ActionVariableType>, // If set we assume the let or const is already parsed
  ) -> Result<ParseActionStateVar, ParsingError> {
    let var_type = if let Some(type_) = var_type_option {
      type_
    } else {
      let to_match = &[(Keywords::Const, " \t\n"), (Keywords::Let, " \t\n")];
      let match_result = self.p.try_match(to_match);
      if let None = match_result {
        return self
          .p
          .unexpected_char(*self.p.contents.get(self.p.index).unwrap() as char);
      }

      if let Keywords::Const = match_result.unwrap() {
        ActionVariableType::Const
      } else {
        ActionVariableType::Let
      }
    };

    let mut res = ParseActionStateVar {
      name: vec![],
      var_type,
      type_: None,
      action: None,
    };

    // Parse name
    let mut next_char = self.p.next_while(" \t\n");
    loop {
      if let Some(c) = next_char {
        match c {
          _ if legal_name_char(c) => res.name.push(c as u8),
          ' ' | '\t' | '\n' => break,
          ':' | '=' => {
            self.p.index -= 1;
            break;
          }
          c => return self.p.unexpected_char(c),
        }
      } else {
        return self.p.unexpected_eof();
      }
      next_char = self.p.next_char();
    }

    // Parse the variable type if set
    next_char = self.p.next_while(" \t\n");
    if let None = next_char {
      return self.p.unexpected_eof();
    }
    if next_char.unwrap() == ':' {
      res.type_ = Some(ParseType::start(self.p, true)?);
      next_char = self.p.next_while(" \t\n");
    }

    // Check for the = symbol
    match next_char {
      Some('=') => {}
      Some(c) => return self.p.unexpected_char(c),
      None => return self.p.unexpected_eof(),
    }

    // Parse the action after the action after the =
    let parsed_action = ParseAction::start(self.p, false, ActionToExpect::Assignment)?;
    res.action = Some(parsed_action);

    Ok(res)
  }
}
