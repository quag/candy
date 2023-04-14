use crate::{
    channel::ChannelId,
    channel::{Capacity, Packet},
    fiber::{Fiber, Status},
    heap::{Closure, Data, Heap, Int, List, Pointer, ReceivePort, SendPort, Struct, Tag, Text},
};
use candy_frontend::builtin_functions::BuiltinFunction;
use itertools::Itertools;
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::ToPrimitive;
use paste::paste;
use std::{ops::Deref, str::FromStr};
use tracing::{info, span, Level};
use unicode_segmentation::UnicodeSegmentation;

impl Fiber {
    pub(super) fn run_builtin_function(
        &mut self,
        builtin_function: &BuiltinFunction,
        args: &[Pointer],
        responsible: Pointer,
    ) {
        let result = span!(Level::TRACE, "Running builtin").in_scope(|| match &builtin_function {
            BuiltinFunction::ChannelCreate => self.heap.channel_create(args),
            BuiltinFunction::ChannelSend => self.heap.channel_send(args),
            BuiltinFunction::ChannelReceive => self.heap.channel_receive(args),
            BuiltinFunction::Equals => self.heap.equals(args),
            BuiltinFunction::FunctionRun => self.heap.function_run(args, responsible),
            BuiltinFunction::GetArgumentCount => self.heap.get_argument_count(args),
            BuiltinFunction::IfElse => self.heap.if_else(args, responsible),
            BuiltinFunction::IntAdd => self.heap.int_add(args),
            BuiltinFunction::IntBitLength => self.heap.int_bit_length(args),
            BuiltinFunction::IntBitwiseAnd => self.heap.int_bitwise_and(args),
            BuiltinFunction::IntBitwiseOr => self.heap.int_bitwise_or(args),
            BuiltinFunction::IntBitwiseXor => self.heap.int_bitwise_xor(args),
            BuiltinFunction::IntCompareTo => self.heap.int_compare_to(args),
            BuiltinFunction::IntDivideTruncating => self.heap.int_divide_truncating(args),
            BuiltinFunction::IntModulo => self.heap.int_modulo(args),
            BuiltinFunction::IntMultiply => self.heap.int_multiply(args),
            BuiltinFunction::IntParse => self.heap.int_parse(args),
            BuiltinFunction::IntRemainder => self.heap.int_remainder(args),
            BuiltinFunction::IntShiftLeft => self.heap.int_shift_left(args),
            BuiltinFunction::IntShiftRight => self.heap.int_shift_right(args),
            BuiltinFunction::IntSubtract => self.heap.int_subtract(args),
            BuiltinFunction::ListFilled => self.heap.list_filled(args),
            BuiltinFunction::ListGet => self.heap.list_get(args),
            BuiltinFunction::ListInsert => self.heap.list_insert(args),
            BuiltinFunction::ListLength => self.heap.list_length(args),
            BuiltinFunction::ListRemoveAt => self.heap.list_remove_at(args),
            BuiltinFunction::ListReplace => self.heap.list_replace(args),
            BuiltinFunction::Parallel => self.heap.parallel(args),
            BuiltinFunction::Print => self.heap.print(args),
            BuiltinFunction::StructGet => self.heap.struct_get(args),
            BuiltinFunction::StructGetKeys => self.heap.struct_get_keys(args),
            BuiltinFunction::StructHasKey => self.heap.struct_has_key(args),
            BuiltinFunction::TagGetSymbol => self.heap.tag_get_symbol(args),
            BuiltinFunction::TagHasValue => self.heap.tag_has_value(args),
            BuiltinFunction::TagGetValue => self.heap.tag_get_value(args),
            BuiltinFunction::TextCharacters => self.heap.text_characters(args),
            BuiltinFunction::TextConcatenate => self.heap.text_concatenate(args),
            BuiltinFunction::TextContains => self.heap.text_contains(args),
            BuiltinFunction::TextEndsWith => self.heap.text_ends_with(args),
            BuiltinFunction::TextFromUtf8 => self.heap.text_from_utf8(args),
            BuiltinFunction::TextGetRange => self.heap.text_get_range(args),
            BuiltinFunction::TextIsEmpty => self.heap.text_is_empty(args),
            BuiltinFunction::TextLength => self.heap.text_length(args),
            BuiltinFunction::TextStartsWith => self.heap.text_starts_with(args),
            BuiltinFunction::TextTrimEnd => self.heap.text_trim_end(args),
            BuiltinFunction::TextTrimStart => self.heap.text_trim_start(args),
            BuiltinFunction::ToDebugText => self.heap.to_debug_text(args),
            BuiltinFunction::Try => self.heap.try_(args),
            BuiltinFunction::TypeOf => self.heap.type_of(args),
        });
        match result {
            Ok(Return(value)) => self.data_stack.push(value),
            Ok(DivergeControlFlow {
                closure,
                responsible,
            }) => self.call(closure, vec![], responsible),
            Ok(CreateChannel { capacity }) => self.status = Status::CreatingChannel { capacity },
            Ok(Send { channel, packet }) => self.status = Status::Sending { channel, packet },
            Ok(Receive { channel }) => self.status = Status::Receiving { channel },
            Ok(Parallel { body }) => self.status = Status::InParallelScope { body },
            Ok(Try { body }) => self.status = Status::InTry { body },
            Err(reason) => self.panic(reason, self.heap.get_hir_id(responsible)),
        }
    }
}

type BuiltinResult = Result<SuccessfulBehavior, String>;
enum SuccessfulBehavior {
    Return(Pointer),
    DivergeControlFlow {
        closure: Pointer,
        responsible: Pointer,
    },
    CreateChannel {
        capacity: Capacity,
    },
    Send {
        channel: ChannelId,
        packet: Packet,
    },
    Receive {
        channel: ChannelId,
    },
    Parallel {
        body: Pointer,
    },
    Try {
        body: Pointer,
    },
}
use SuccessfulBehavior::*;

impl From<SuccessfulBehavior> for BuiltinResult {
    fn from(ok: SuccessfulBehavior) -> Self {
        Ok(ok)
    }
}

macro_rules! unpack {
    ( $heap:expr, $args:expr, |$( $arg:ident: $type:ty ),+| $body:block ) => {
        {
            let ( $( $arg, )+ ) = if let [$( $arg, )+] = $args {
                ( $( *$arg, )+ )
            } else {
                return Err(
                    "A builtin function was called with the wrong number of arguments.".to_string(),
                );
            };
            let ( $( $arg, )+ ): ( $( UnpackedData<$type>, )+ ) = ( $(
                UnpackedData {
                    address: $arg,
                    data: (&$heap.get($arg).data).try_into()?,
                },
            )+ );

            $body.into()
        }
    };
}
macro_rules! unpack_and_later_drop {
    ( $heap:expr, $args:expr, |$( $arg:ident: $type:ty ),+| $body:block ) => {
        {
            let ( $( $arg, )+ ) = if let [$( $arg, )+] = $args {
                ( $( *$arg, )+ )
            } else {
                return Err(
                    "A builtin function was called with the wrong number of arguments.".to_string(),
                );
            };
            let ( $( $arg, )+ ): ( $( UnpackedData<$type>, )+ ) = ( $(
                UnpackedData {
                    address: $arg,
                    data: (&$heap.get($arg).data).try_into()?,
                },
            )+ );

            // Structs are called `struct_`, so we sometimes generate
            // identifiers containing a double underscore.
            #[allow(non_snake_case)]
            $( let paste!([< $arg _address >]) = $arg.address; )+

            let result = $body;

            $( $heap.drop(paste!([< $arg _address >])); )+
            result.into()
        }
    };
}

impl Heap {
    fn channel_create(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |capacity: &Int| {
            match capacity.value.clone().try_into() {
                Ok(capacity) => CreateChannel { capacity },
                Err(_) => return Err("You tried to create a channel with a capacity that is either negative or bigger than the maximum usize.".to_string()),
            }
        })
    }

    fn channel_send(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |port: &SendPort, packet: Any| {
            let mut heap = Heap::default();
            let address = self.clone_single_to_other_heap(&mut heap, packet.address);
            Send {
                channel: port.channel,
                packet: Packet { heap, address },
            }
        })
    }

    fn channel_receive(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |port: &ReceivePort| {
            Receive {
                channel: port.channel,
            }
        })
    }

    fn equals(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |a: Any, b: Any| {
            let is_equal = a.equals(self, &b);
            Return(self.create_bool(is_equal))
        })
    }

    fn function_run(&mut self, args: &[Pointer], responsible: Pointer) -> BuiltinResult {
        unpack!(self, args, |closure: &Closure| {
            closure.should_take_no_arguments()?;
            DivergeControlFlow {
                closure: closure.address,
                responsible,
            }
        })
    }

    fn get_argument_count(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |closure: &Closure| {
            let num_args = closure.num_args.into();
            Return(self.create_int(num_args))
        })
    }

    fn if_else(&mut self, args: &[Pointer], responsible: Pointer) -> BuiltinResult {
        unpack!(self, args, |condition: bool,
                             then: &Closure,
                             else_: &Closure| {
            let (run, dont_run) = if *condition {
                (then, else_)
            } else {
                (else_, then)
            };

            let condition_address = condition.address;
            let run_address = run.address;
            let dont_run_address = dont_run.address;
            self.drop(condition_address);
            self.drop(dont_run_address);

            DivergeControlFlow {
                closure: run_address,
                responsible,
            }
        })
    }

    fn int_add(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |a: &Int, b: &Int| {
            Return(self.create_int(&a.value + &b.value))
        })
    }
    fn int_bit_length(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |a: &Int| {
            Return(self.create_int(a.value.bits().into()))
        })
    }
    fn int_bitwise_and(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |a: &Int, b: &Int| {
            Return(self.create_int(&a.value & &b.value))
        })
    }
    fn int_bitwise_or(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |a: &Int, b: &Int| {
            Return(self.create_int(&a.value | &b.value))
        })
    }
    fn int_bitwise_xor(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |a: &Int, b: &Int| {
            Return(self.create_int(&a.value ^ &b.value))
        })
    }
    fn int_compare_to(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |a: &Int, b: &Int| {
            Return(self.create_ordering(a.value.cmp(&b.value)))
        })
    }
    fn int_divide_truncating(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |dividend: &Int, divisor: &Int| {
            if divisor.data.value == 0.into() {
                return Err("Can't divide by zero.".to_string());
            }
            Return(self.create_int(&dividend.value / &divisor.value))
        })
    }
    fn int_modulo(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |dividend: &Int, divisor: &Int| {
            if divisor.data.value == 0.into() {
                return Err("Can't divide by zero.".to_string());
            }
            Return(self.create_int(dividend.value.mod_floor(&divisor.value)))
        })
    }
    fn int_multiply(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |factor_a: &Int, factor_b: &Int| {
            Return(self.create_int(&factor_a.value * &factor_b.value))
        })
    }
    fn int_parse(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text| {
            let result = match BigInt::from_str(&text.value) {
                Ok(int) => Ok(self.create_int(int)),
                Err(err) => Err(self.create_text(format!("{err}"))),
            };
            Return(self.create_result(result))
        })
    }
    fn int_remainder(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |dividend: &Int, divisor: &Int| {
            if divisor.data.value == 0.into() {
                return Err("Can't divide by zero.".to_string());
            }
            Return(self.create_int(&dividend.value % &divisor.value))
        })
    }
    fn int_shift_left(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |value: &Int, amount: &Int| {
            let amount = amount.value.to_u128().unwrap();
            Return(self.create_int(&value.value << amount))
        })
    }
    fn int_shift_right(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |value: &Int, amount: &Int| {
            let amount = amount.value.to_u128().unwrap();
            Return(self.create_int(&value.value >> amount))
        })
    }
    fn int_subtract(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |minuend: &Int, subtrahend: &Int| {
            Return(self.create_int(&minuend.value - &subtrahend.value))
        })
    }

    fn list_filled(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |length: &Int, item: Any| {
            let length = length.value.to_usize().unwrap();
            let item_address = item.address;
            self.dup_by(item.address, length);
            Return(self.create_list(vec![item_address; length]))
        })
    }
    fn list_get(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |list: &List, index: &Int| {
            let index = index.value.to_usize().unwrap();
            let item = list.items[index];
            self.dup(item);
            Return(item)
        })
    }
    fn list_insert(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |list: &List, index: &Int, item: Any| {
            let mut new_list = list.items.clone();

            let index = index.value.to_usize().unwrap();
            let item_address = item.address;
            self.dup(item.address);
            new_list.insert(index, item_address);

            Return(self.create_list(new_list))
        })
    }
    fn list_length(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |list: &List| {
            Return(self.create_int(list.items.len().into()))
        })
    }
    fn list_remove_at(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |list: &List, index: &Int| {
            let mut new_list = list.items.clone();

            let index = index.value.to_usize().unwrap();
            new_list.remove(index);

            Return(self.create_list(new_list))
        })
    }
    fn list_replace(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |list: &List, index: &Int, new_item: Any| {
            let mut new_list = list.items.clone();

            let index = index.value.to_usize().unwrap();
            let new_item_address = new_item.address;
            self.dup(new_item.address);
            new_list[index] = new_item_address;

            Return(self.create_list(new_list))
        })
    }

    fn parallel(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack!(self, args, |body_taking_nursery: &Closure| {
            if body_taking_nursery.num_args != 1 {
                return Err("`parallel` expects a closure taking a nursery.".to_string());
            }
            Parallel {
                body: body_taking_nursery.address,
            }
        })
    }

    fn print(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |message: Any| {
            info!("{}", message.address.format(self));
            Return(self.create_nothing())
        })
    }

    fn struct_get(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |struct_: &Struct, key: Any| {
            match struct_.get(self, key.address) {
                Some(value) => {
                    self.dup(value);
                    Ok(Return(value))
                }
                None => Err(format!(
                    "The struct does not contain the key {}.",
                    key.address.format(self),
                )),
            }
        })
    }
    fn struct_get_keys(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |struct_: &Struct| {
            Return(self.create_list(struct_.iter().map(|(key, _)| key).collect_vec()))
        })
    }
    fn struct_has_key(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |struct_: &Struct, key: Any| {
            let has_key = struct_.get(self, key.address).is_some();
            Return(self.create_bool(has_key))
        })
    }

    fn tag_get_symbol(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |tag: &Tag| {
            Return(self.create_tag(tag.symbol.to_string(), None))
        })
    }
    fn tag_has_value(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |tag: &Tag| {
            Return(self.create_bool(tag.value.is_some()))
        })
    }
    fn tag_get_value(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |tag: &Tag| {
            tag.value
                .map_or(Err("The tag doesn't have a value.".to_string()), |value| {
                    self.dup(value);
                    Ok(Return(value))
                })
        })
    }

    fn text_characters(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text| {
            let text = text.value.clone();
            let character_addresses = text
                .graphemes(true)
                .map(|it| self.create_text(it.to_string()))
                .collect_vec();
            Return(self.create_list(character_addresses))
        })
    }
    fn text_concatenate(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text_a: &Text, text_b: &Text| {
            Return(self.create_text(format!("{}{}", text_a.value, text_b.value)))
        })
    }
    fn text_contains(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text, pattern: &Text| {
            Return(self.create_bool(text.value.contains(&pattern.value)))
        })
    }
    fn text_ends_with(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text, suffix: &Text| {
            Return(self.create_bool(text.value.ends_with(&suffix.value)))
        })
    }
    fn text_from_utf8(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |bytes: &List| {
            let bytes = bytes
                .items
                .iter()
                .map(|&it| {
                    let int: Int = self.get(it).data.clone().try_into()?;
                    int.value
                        .to_u8()
                        .ok_or_else(|| format!("Number is not a byte: {}.", int.value))
                })
                .try_collect()?;
            let result = String::from_utf8(bytes)
                .map(|string| self.create_text(string))
                .map_err(|_| self.create_text("Invalid UTF-8.".to_string()));
            Return(self.create_result(result))
        })
    }
    fn text_get_range(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(
            self,
            args,
            |text: &Text, start_inclusive: &Int, end_exclusive: &Int| {
                let start_inclusive = start_inclusive.value.to_usize().expect(
                    "Tried to get a range from a text with an index that's too large for usize.",
                );
                let end_exclusive = end_exclusive.value.to_usize().expect(
                    "Tried to get a range from a text with an index that's too large for usize.",
                );
                let text = text
                    .value
                    .graphemes(true)
                    .skip(start_inclusive)
                    .take(end_exclusive - start_inclusive)
                    .collect();
                Return(self.create_text(text))
            }
        )
    }
    fn text_is_empty(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text| {
            Return(self.create_bool(text.value.is_empty()))
        })
    }
    fn text_length(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text| {
            let length = text.value.graphemes(true).count().into();
            Return(self.create_int(length))
        })
    }
    fn text_starts_with(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text, prefix: &Text| {
            Return(self.create_bool(text.value.starts_with(&prefix.value)))
        })
    }
    fn text_trim_end(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text| {
            Return(self.create_text(text.value.trim_end().to_string()))
        })
    }
    fn text_trim_start(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |text: &Text| {
            Return(self.create_text(text.value.trim_start().to_string()))
        })
    }

    #[allow(clippy::wrong_self_convention)]
    fn to_debug_text(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |value: Any| {
            Return(self.create_text(value.address.format(self)))
        })
    }

    fn try_(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack!(self, args, |body: &Closure| { Try { body: body.address } })
    }

    fn type_of(&mut self, args: &[Pointer]) -> BuiltinResult {
        unpack_and_later_drop!(self, args, |value: Any| {
            // FIXME: Change symbol to tag
            let symbol = match **value {
                Data::Int(_) => "Int",
                Data::Text(_) => "Text",
                Data::Tag(_) => "Symbol",
                Data::List(_) => "List",
                Data::Struct(_) => "Struct",
                Data::HirId(_) => unreachable!(),
                Data::Closure(_) => "Function",
                Data::Builtin(_) => "Builtin",
                Data::SendPort(_) => "SendPort",
                Data::ReceivePort(_) => "ReceivePort",
            };
            Return(self.create_tag(symbol.to_string(), None))
        })
    }
}

impl Closure {
    fn should_take_no_arguments(&self) -> Result<(), String> {
        match self.num_args {
            0 => Ok(()),
            n => Err(format!("A builtin function expected a function without arguments, but got one that takes {n} arguments.")),
        }
    }
}

struct UnpackedData<T> {
    address: Pointer,
    data: T,
}
impl<T> Deref for UnpackedData<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

struct Any<'a> {
    data: &'a Data,
}
impl<'a> Deref for Any<'a> {
    type Target = Data;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a> TryInto<Any<'a>> for &'a Data {
    type Error = String;

    fn try_into(self) -> Result<Any<'a>, Self::Error> {
        Ok(Any { data: self })
    }
}
