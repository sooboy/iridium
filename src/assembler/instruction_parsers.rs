use std::io::Cursor;

use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian, LittleEndian, ByteOrder};
use nom::types::CompleteStr;

use assembler::label_parsers::label_declaration;
use assembler::opcode_parsers::*;
use assembler::operand_parsers::operand;
use assembler::comment_parsers::comment;
use assembler::{SymbolTable, Token};
use instruction::Opcode;

const MAX_I16: i32 = 32768;
const MIN_I16: i32 = -32768;

#[derive(Debug, PartialEq)]
pub struct AssemblerInstruction {
    pub opcode: Option<Token>,
    pub label: Option<Token>,
    pub directive: Option<Token>,
    pub operand1: Option<Token>,
    pub operand2: Option<Token>,
    pub operand3: Option<Token>,
}

impl AssemblerInstruction {
    pub fn to_bytes(&self, symbols: &SymbolTable) -> Vec<u8> {
        let mut results = vec![];
        if let Some(ref token) = self.opcode {
            match token {
                Token::Op { code } => match code {
                    _ => {
                        results.push(*code as u8);
                    }
                },
                _ => {
                    println!("Non-opcode found in opcode field");
                }
            }
        }

        for operand in &[&self.operand1, &self.operand2, &self.operand3] {
            if let Some(token) = operand {
                AssemblerInstruction::extract_operand(token, &mut results, symbols)
            }
        }

        while results.len() < 4 {
            results.push(0);
        }

        results
    }

    pub fn is_label(&self) -> bool {
        self.label.is_some()
    }

    pub fn is_opcode(&self) -> bool {
        self.opcode.is_some()
    }

    pub fn is_directive(&self) -> bool {
        self.directive.is_some()
    }

    /// Checks if the AssemblyInstruction has any operands at all
    pub fn has_operands(&self) -> bool {
        self.operand1.is_some() || self.operand2.is_some() || self.operand3.is_some()
    }

    pub fn get_directive_name(&self) -> Option<String> {
        match &self.directive {
            Some(d) => match d {
                Token::Directive { name } => Some(name.to_string()),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_string_constant(&self) -> Option<String> {
        match &self.operand1 {
            Some(d) => match d {
                Token::IrString { name } => Some(name.to_string()),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_i32_constant(&self) -> Option<i32> {
        match &self.operand1 {
            Some(d) => match d {
                Token::IntegerOperand { value } => Some(*value),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_label_name(&self) -> Option<String> {
        match &self.label {
            Some(l) => match l {
                Token::LabelDeclaration { name } => Some(name.clone()),
                _ => None,
            },
            None => None,
        }
    }

    fn extract_operand(t: &Token, results: &mut Vec<u8>, symbols: &SymbolTable) {
        match t {
            Token::Register { reg_num } => {
                results.push(*reg_num);
            }
            // This operand is a bit special. Since we use fixed width instructions, we only have 16-bits to use for the number.
            // If the user wants to store a 32-bit register, we need to convert the number into bits, and then use two instructions to
            // get the entire value into the register
            Token::IntegerOperand { value } => {
                if *value > MAX_I16 || *value < MIN_I16 {
                    // This creates the second instructino that loads the second group of 16 bits
                    let mut wtr = vec![];
                    wtr.write_i32::<LittleEndian>(*value).unwrap();
                    results.push(wtr[3]);
                    results.push(wtr[2]);

                    let opcode: u8 = 39;
                    let register_offset = results.len() - 3;
                    let register = results[register_offset];
                    results.push(opcode);
                    results.push(register);
                    results.push(wtr[1]);
                    results.push(wtr[0]);

                } else {
                    let byte1 = value >> 8;
                    let byte2 = value >> 8;
                    results.push(byte2 as u8);
                    results.push(byte1 as u8);
                }
            }
            Token::LabelUsage { name } => {
                if let Some(value) = symbols.symbol_value(name) {
                    let byte1 = value >> 8;
                    let byte2 = value >> 8;
                    results.push(byte2 as u8);
                    results.push(byte1 as u8);
                }
            }
            _ => {
                println!("Opcode found in operand field: {:#?}", t);
            }
        };
    }
}

named!(instruction_combined<CompleteStr, AssemblerInstruction>,
    do_parse!(
        opt!(comment) >>
        l: opt!(label_declaration) >>
        o: opcode >>
        opt!(comment) >>
        o1: opt!(operand) >>
        o2: opt!(operand) >>
        o3: opt!(operand) >>
        opt!(comment) >>
        (
            AssemblerInstruction{
                opcode: Some(o),
                label: l,
                directive: None,
                operand1: o1,
                operand2: o2,
                operand3: o3,
            }
        )
    )
);

/// Will try to parse out any of the Instruction forms
named!(pub instruction<CompleteStr, AssemblerInstruction>,
    do_parse!(
        ins: alt!(
            instruction_combined
        ) >>
        (
            ins
        )
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use instruction::Opcode;

    #[test]
    fn test_parse_instruction_form_one() {
        let result = instruction_combined(CompleteStr("load $0 #100\n"));
        assert_eq!(
            result,
            Ok((
                CompleteStr(""),
                AssemblerInstruction {
                    opcode: Some(Token::Op { code: Opcode::LOAD }),
                    label: None,
                    directive: None,
                    operand1: Some(Token::Register { reg_num: 0 }),
                    operand2: Some(Token::IntegerOperand { value: 100 }),
                    operand3: None
                }
            ))
        );
    }

    #[test]
    fn test_parse_instruction_form_one_with_label() {
        let result = instruction_combined(CompleteStr("load $0 @test1\n"));
        assert_eq!(
            result,
            Ok((
                CompleteStr(""),
                AssemblerInstruction {
                    opcode: Some(Token::Op { code: Opcode::LOAD }),
                    label: None,
                    directive: None,
                    operand1: Some(Token::Register { reg_num: 0 }),
                    operand2: Some(Token::LabelUsage {
                        name: "test1".to_string()
                    }),
                    operand3: None
                }
            ))
        );
    }

    #[test]
    fn test_parse_instruction_form_two() {
        let result = instruction_combined(CompleteStr("hlt"));
        assert_eq!(
            result,
            Ok((
                CompleteStr(""),
                AssemblerInstruction {
                    opcode: Some(Token::Op { code: Opcode::HLT }),
                    label: None,
                    directive: None,
                    operand1: None,
                    operand2: None,
                    operand3: None
                }
            ))
        );
    }

    #[test]
    fn test_parse_instruction_form_three() {
        let result = instruction_combined(CompleteStr("add $0 $1 $2\n"));
        assert_eq!(
            result,
            Ok((
                CompleteStr(""),
                AssemblerInstruction {
                    opcode: Some(Token::Op { code: Opcode::ADD }),
                    label: None,
                    directive: None,
                    operand1: Some(Token::Register { reg_num: 0 }),
                    operand2: Some(Token::Register { reg_num: 1 }),
                    operand3: Some(Token::Register { reg_num: 2 }),
                }
            ))
        );
    }

    #[test]
    fn test_parse_instruction_with_comment_one() {
        let result = instruction_combined(CompleteStr("; this is a test\nadd $0 $1 $2\n"));
        assert_eq!(
            result,
            Ok((
                CompleteStr(""),
                AssemblerInstruction {
                    opcode: Some(Token::Op { code: Opcode::ADD }),
                    label: None,
                    directive: None,
                    operand1: Some(Token::Register { reg_num: 0 }),
                    operand2: Some(Token::Register { reg_num: 1 }),
                    operand3: Some(Token::Register { reg_num: 2 }),
                }
            ))
        );
    }

    #[test]
    fn test_parse_instruction_with_comment_two() {
        let result = instruction_combined(CompleteStr("add $0 $1 $2 ; this is a test\n"));
        assert_eq!(
            result,
            Ok((
                CompleteStr(""),
                AssemblerInstruction {
                    opcode: Some(Token::Op { code: Opcode::ADD }),
                    label: None,
                    directive: None,
                    operand1: Some(Token::Register { reg_num: 0 }),
                    operand2: Some(Token::Register { reg_num: 1 }),
                    operand3: Some(Token::Register { reg_num: 2 }),
                }
            ))
        );
    }
}
