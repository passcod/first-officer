/// Parse text to extract thinking blocks wrapped in <thinking>...</thinking> tags.
///
/// Returns a vector of content blocks. If the text contains valid thinking tags,
/// they're extracted as separate ThinkingBlock entries. All other text becomes
/// TextBlock entries. If no thinking tags are found, the entire text is returned
/// as a single TextBlock.
use crate::translate::types::{AssistantContentBlock, TextBlock, ThinkingBlock};

/// Parse assistant message text and extract thinking blocks.
///
/// Looks for `<thinking>...</thinking>` tags and splits the content accordingly.
/// If no thinking tags are found, returns the entire text as a single TextBlock.
pub fn parse_thinking_blocks(text: &str) -> Vec<AssistantContentBlock> {
	let mut blocks = Vec::new();
	let mut remaining = text;
	let mut found_thinking = false;

	while let Some(start_idx) = remaining.find("<thinking>") {
		found_thinking = true;

		// Text before the thinking tag
		let prefix = &remaining[..start_idx];
		if !prefix.trim().is_empty() {
			blocks.push(AssistantContentBlock::Text(TextBlock {
				text: prefix.to_string(),
			}));
		}

		// Find the closing tag
		let after_open = &remaining[start_idx + "<thinking>".len()..];
		if let Some(end_idx) = after_open.find("</thinking>") {
			let thinking_content = &after_open[..end_idx];
			blocks.push(AssistantContentBlock::Thinking(ThinkingBlock {
				thinking: thinking_content.to_string(),
			}));

			// Continue with text after the closing tag
			remaining = &after_open[end_idx + "</thinking>".len()..];
		} else {
			// Unclosed thinking tag - treat the rest as text
			blocks.push(AssistantContentBlock::Text(TextBlock {
				text: remaining.to_string(),
			}));
			remaining = "";
			break;
		}
	}

	// Add any remaining text after the last thinking block
	if !remaining.is_empty() {
		blocks.push(AssistantContentBlock::Text(TextBlock {
			text: remaining.to_string(),
		}));
	}

	// If no thinking blocks were found, return original text as single block
	if !found_thinking {
		return vec![AssistantContentBlock::Text(TextBlock {
			text: text.to_string(),
		})];
	}

	blocks
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn no_thinking_tags() {
		let text = "Just a regular response.";
		let blocks = parse_thinking_blocks(text);
		assert_eq!(blocks.len(), 1);
		assert!(matches!(&blocks[0], AssistantContentBlock::Text(t) if t.text == text));
	}

	#[test]
	fn single_thinking_block() {
		let text = "<thinking>Let me think...</thinking>The answer is 42.";
		let blocks = parse_thinking_blocks(text);
		assert_eq!(blocks.len(), 2);
		assert!(
			matches!(&blocks[0], AssistantContentBlock::Thinking(t) if t.thinking == "Let me think...")
		);
		assert!(
			matches!(&blocks[1], AssistantContentBlock::Text(t) if t.text == "The answer is 42.")
		);
	}

	#[test]
	fn thinking_only() {
		let text = "<thinking>Just thinking, no answer</thinking>";
		let blocks = parse_thinking_blocks(text);
		assert_eq!(blocks.len(), 1);
		assert!(
			matches!(&blocks[0], AssistantContentBlock::Thinking(t) if t.thinking == "Just thinking, no answer")
		);
	}

	#[test]
	fn text_before_and_after() {
		let text = "Before<thinking>thinking</thinking>After";
		let blocks = parse_thinking_blocks(text);
		assert_eq!(blocks.len(), 3);
		assert!(matches!(&blocks[0], AssistantContentBlock::Text(t) if t.text == "Before"));
		assert!(
			matches!(&blocks[1], AssistantContentBlock::Thinking(t) if t.thinking == "thinking")
		);
		assert!(matches!(&blocks[2], AssistantContentBlock::Text(t) if t.text == "After"));
	}

	#[test]
	fn multiple_thinking_blocks() {
		let text = "<thinking>First</thinking>Middle<thinking>Second</thinking>End";
		let blocks = parse_thinking_blocks(text);
		assert_eq!(blocks.len(), 4);
		assert!(matches!(&blocks[0], AssistantContentBlock::Thinking(t) if t.thinking == "First"));
		assert!(matches!(&blocks[1], AssistantContentBlock::Text(t) if t.text == "Middle"));
		assert!(matches!(&blocks[2], AssistantContentBlock::Thinking(t) if t.thinking == "Second"));
		assert!(matches!(&blocks[3], AssistantContentBlock::Text(t) if t.text == "End"));
	}

	#[test]
	fn unclosed_thinking_tag() {
		let text = "<thinking>This is never closed";
		let blocks = parse_thinking_blocks(text);
		assert_eq!(blocks.len(), 1);
		assert!(matches!(&blocks[0], AssistantContentBlock::Text(t) if t.text == text));
	}

	#[test]
	fn whitespace_only_between_blocks() {
		let text = "<thinking>Think</thinking>   \n\t  <thinking>More</thinking>";
		let blocks = parse_thinking_blocks(text);
		// Whitespace-only text blocks are filtered out
		assert_eq!(blocks.len(), 2);
		assert!(matches!(&blocks[0], AssistantContentBlock::Thinking(t) if t.thinking == "Think"));
		assert!(matches!(&blocks[1], AssistantContentBlock::Thinking(t) if t.thinking == "More"));
	}
}

/// Events emitted by the streaming thinking parser.
pub enum ThinkingEvent {
	/// Start of a thinking block - open a new thinking content block
	ThinkingStart,
	/// Delta of thinking content - emit as thinking delta
	ThinkingDelta(String),
	/// End of a thinking block - close the thinking content block
	ThinkingEnd,
	/// Text content delta - emit as regular text delta
	TextDelta(String),
}

/// Streaming parser for extracting thinking blocks incrementally.
///
/// Emits events as thinking tags are detected for immediate streaming.
/// When inside a thinking block, all text is emitted as thinking deltas.
/// Text outside thinking blocks is emitted as text deltas.
pub struct ThinkingStreamParser {
	buffer: String,
	in_thinking: bool,
}

impl ThinkingStreamParser {
	pub fn new() -> Self {
		Self {
			buffer: String::new(),
			in_thinking: false,
		}
	}

	/// Process a chunk of text and return emitted events.
	///
	/// Returns a vector of events that should be processed by the stream handler.
	pub fn push(&mut self, chunk: &str) -> Vec<ThinkingEvent> {
		self.buffer.push_str(chunk);
		let mut events = Vec::new();

		loop {
			if self.in_thinking {
				// Inside a thinking block - look for closing tag
				if let Some(end_idx) = self.buffer.find("</thinking>") {
					// Emit any buffered thinking content
					if end_idx > 0 {
						let thinking_content = self.buffer[..end_idx].to_string();
						events.push(ThinkingEvent::ThinkingDelta(thinking_content));
					}

					// Signal end of thinking block
					events.push(ThinkingEvent::ThinkingEnd);

					// Remove the thinking content and closing tag from buffer
					self.buffer.drain(..end_idx + "</thinking>".len());
					self.in_thinking = false;
				} else {
					// Still inside thinking block - emit buffered content as delta,
					// but keep a reserve in case closing tag is split across chunks
					let reserve = "</thinking>".len().min(self.buffer.len());
					if self.buffer.len() > reserve {
						let emit_len = self.buffer.len() - reserve;
						let to_emit = self.buffer[..emit_len].to_string();
						if !to_emit.is_empty() {
							events.push(ThinkingEvent::ThinkingDelta(to_emit));
						}
						self.buffer.drain(..emit_len);
					}
					break;
				}
			} else {
				// Outside thinking block - look for opening tag
				if let Some(start_idx) = self.buffer.find("<thinking>") {
					// Emit any text before the tag
					if start_idx > 0 {
						let prefix = self.buffer[..start_idx].to_string();
						if !prefix.is_empty() {
							events.push(ThinkingEvent::TextDelta(prefix));
						}
					}

					// Signal start of thinking block
					events.push(ThinkingEvent::ThinkingStart);

					// Remove the text and opening tag from buffer
					self.buffer.drain(..start_idx + "<thinking>".len());
					self.in_thinking = true;
				} else {
					// No thinking tag found - emit buffered text as delta,
					// but keep a reserve in case opening tag is split across chunks
					let reserve = "<thinking>".len().min(self.buffer.len());
					if self.buffer.len() > reserve {
						let emit_len = self.buffer.len() - reserve;
						let to_emit = self.buffer[..emit_len].to_string();
						if !to_emit.is_empty() {
							events.push(ThinkingEvent::TextDelta(to_emit));
						}
						self.buffer.drain(..emit_len);
					}
					break;
				}
			}
		}

		events
	}

	/// Flush any remaining buffered content.
	///
	/// Call this when the stream is complete to emit any final text.
	/// Returns an event for the remaining content (either thinking or text).
	pub fn finish(self) -> Option<ThinkingEvent> {
		if !self.buffer.is_empty() {
			if self.in_thinking {
				Some(ThinkingEvent::ThinkingDelta(self.buffer))
			} else {
				Some(ThinkingEvent::TextDelta(self.buffer))
			}
		} else {
			None
		}
	}
}

#[cfg(test)]
mod streaming_tests {
	use super::*;

	#[test]
	fn stream_simple_text() {
		let mut parser = ThinkingStreamParser::new();
		let events = parser.push("Hello ");
		// Reserve buffer is 10 chars, "Hello " is only 6, so nothing emitted yet
		assert_eq!(events.len(), 0);

		let events = parser.push("world");
		// Now we have 11 chars total, emit all but last 10 (reserve)
		assert_eq!(events.len(), 1);
		assert!(matches!(&events[0], ThinkingEvent::TextDelta(s) if s == "H"));

		let final_event = parser.finish();
		assert!(matches!(final_event, Some(ThinkingEvent::TextDelta(s)) if s == "ello world"));
	}

	#[test]
	fn stream_thinking_block() {
		let mut parser = ThinkingStreamParser::new();
		let events = parser.push("<thinking>Let me ");
		// Opens thinking, "Let me " buffered (reserve 12 chars)
		assert_eq!(events.len(), 1);
		assert!(matches!(&events[0], ThinkingEvent::ThinkingStart));

		let events = parser.push("think...</thinking>Answer");
		// Emits buffered thinking, closes, "Answer" starts buffering
		assert_eq!(events.len(), 2);
		assert!(matches!(&events[0], ThinkingEvent::ThinkingDelta(s) if s == "Let me think..."));
		assert!(matches!(&events[1], ThinkingEvent::ThinkingEnd));

		let events = parser.push(" is 42");
		// "Answer is 42" = 12 chars, reserve is 10, emit first 2
		assert_eq!(events.len(), 1);
		assert!(matches!(&events[0], ThinkingEvent::TextDelta(s) if s == "An"));

		let final_event = parser.finish();
		assert!(matches!(final_event, Some(ThinkingEvent::TextDelta(s)) if s == "swer is 42"));
	}

	#[test]
	fn stream_tag_split_across_chunks() {
		let mut parser = ThinkingStreamParser::new();
		let events = parser.push("Text <thin");
		// "Text <thin" = 10 chars, reserve is 10, nothing emitted
		assert_eq!(events.len(), 0);

		let events = parser.push("king>inside</thinking>after");
		// Completes tag, emits "Text ", opens thinking, emits "inside", closes thinking
		assert_eq!(events.len(), 4);
		assert!(matches!(&events[0], ThinkingEvent::TextDelta(s) if s == "Text "));
		assert!(matches!(&events[1], ThinkingEvent::ThinkingStart));
		assert!(matches!(&events[2], ThinkingEvent::ThinkingDelta(s) if s == "inside"));
		assert!(matches!(&events[3], ThinkingEvent::ThinkingEnd));
	}

	#[test]
	fn stream_multiple_thinking_blocks() {
		let mut parser = ThinkingStreamParser::new();
		let events = parser.push("<thinking>A</thinking>B<thinking>C</thinking>D");
		// All processed in one go since complete tags are present
		assert_eq!(events.len(), 7);
		assert!(matches!(&events[0], ThinkingEvent::ThinkingStart));
		assert!(matches!(&events[1], ThinkingEvent::ThinkingDelta(s) if s == "A"));
		assert!(matches!(&events[2], ThinkingEvent::ThinkingEnd));
		assert!(matches!(&events[3], ThinkingEvent::TextDelta(s) if s == "B"));
		assert!(matches!(&events[4], ThinkingEvent::ThinkingStart));
		assert!(matches!(&events[5], ThinkingEvent::ThinkingDelta(s) if s == "C"));
		assert!(matches!(&events[6], ThinkingEvent::ThinkingEnd));

		let final_event = parser.finish();
		assert!(matches!(final_event, Some(ThinkingEvent::TextDelta(s)) if s == "D"));
	}

	#[test]
	fn stream_thinking_deltas_incrementally() {
		let mut parser = ThinkingStreamParser::new();
		let events = parser.push("<thinking>First ");
		// Opens thinking, "First " buffered (7 chars, reserve is 12)
		assert_eq!(events.len(), 1);
		assert!(matches!(&events[0], ThinkingEvent::ThinkingStart));

		let events = parser.push("second ");
		// "First second " = 14 chars, reserve 12, emit first 2
		assert_eq!(events.len(), 1);
		assert!(matches!(&events[0], ThinkingEvent::ThinkingDelta(s) if s == "Fi"));

		let events = parser.push("third</thinking>");
		// Emit remaining buffered, then close
		assert_eq!(events.len(), 2);
		assert!(matches!(&events[0], ThinkingEvent::ThinkingDelta(s) if s == "rst second third"));
		assert!(matches!(&events[1], ThinkingEvent::ThinkingEnd));
	}
}
