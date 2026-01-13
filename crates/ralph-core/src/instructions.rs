//! Instruction builder for prepending orchestration context to prompts.

use ralph_proto::Hat;

/// Builds the prepended instructions for agent prompts.
#[derive(Debug)]
pub struct InstructionBuilder {
    completion_promise: String,
}

impl InstructionBuilder {
    /// Creates a new instruction builder.
    pub fn new(completion_promise: impl Into<String>) -> Self {
        Self {
            completion_promise: completion_promise.into(),
        }
    }

    /// Builds single-hat mode instructions.
    pub fn build_single_hat(&self, prompt_content: &str) -> String {
        format!(
            r#"You are in a loop. Study PROMPT.md first.

1. Study the implementation plan - don't assume tasks aren't implemented
2. Select the most important incomplete task
3. Implement it fully with tests
4. Run validation (tests, build, lint)
5. Update the plan marking completed work
6. Commit changes
7. Exit

When ALL tasks are complete, output exactly: {promise}

---
{prompt}"#,
            promise = self.completion_promise,
            prompt = prompt_content
        )
    }

    /// Builds multi-hat mode instructions for a specific hat.
    pub fn build_multi_hat(&self, hat: &Hat, events_context: &str) -> String {
        let mut instructions = String::new();

        instructions.push_str("ORCHESTRATION CONTEXT:\n");
        instructions.push_str(&format!("You are the {} agent in a multi-agent system.\n\n", hat.name));

        if !hat.instructions.is_empty() {
            instructions.push_str("YOUR ROLE:\n");
            instructions.push_str(&hat.instructions);
            instructions.push_str("\n\n");
        }

        instructions.push_str("EVENT COMMUNICATION:\n");
        instructions.push_str("Use <event> tags to communicate with other agents:\n");
        instructions.push_str(r#"<event topic="your.topic">Your message</event>"#);
        instructions.push_str("\n\n");

        if !hat.publishes.is_empty() {
            instructions.push_str("You typically publish to: ");
            let topics: Vec<&str> = hat.publishes.iter().map(|t| t.as_str()).collect();
            instructions.push_str(&topics.join(", "));
            instructions.push_str("\n\n");
        }

        instructions.push_str(&format!(
            "COMPLETION:\nWhen the overall task is complete, output:\n{}\n\n",
            self.completion_promise
        ));

        instructions.push_str("---\nINCOMING EVENTS:\n");
        instructions.push_str(events_context);

        instructions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_hat_instructions() {
        let builder = InstructionBuilder::new("LOOP_COMPLETE");
        let instructions = builder.build_single_hat("Implement feature X");

        assert!(instructions.contains("LOOP_COMPLETE"));
        assert!(instructions.contains("Implement feature X"));
        assert!(instructions.contains("Study"));
        assert!(instructions.contains("don't assume"));
    }

    #[test]
    fn test_multi_hat_instructions() {
        let builder = InstructionBuilder::new("DONE");
        let hat = Hat::new("impl", "Implementer")
            .with_instructions("Write clean, tested code.");

        let instructions = builder.build_multi_hat(&hat, "Event: task.start - Begin work");

        assert!(instructions.contains("Implementer agent"));
        assert!(instructions.contains("Write clean, tested code"));
        assert!(instructions.contains("DONE"));
        assert!(instructions.contains("task.start"));
    }
}
