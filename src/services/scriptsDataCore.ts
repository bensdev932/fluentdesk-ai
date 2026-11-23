import type { ScriptTier } from './preConversationScripts';

export interface ScriptDialogueLine {
  speaker: 'Tutor' | 'You';
  text: string;
}

export interface PreConversationScript {
  summary: string;
  lines: ScriptDialogueLine[];
  quickReplies: string[];
}

export const CORE_SCRIPTS: Record<string, Record<ScriptTier, PreConversationScript>> = {
  free_conversation: {
    'A1-A2': {
      summary: 'Simple warm-up chat about your day and hobbies.',
      lines: [
        { speaker: 'Tutor', text: "Hi! How's your day going?" },
        { speaker: 'You', text: "It's going well, thanks! How about you?" },
        { speaker: 'Tutor', text: 'Pretty good! Did you do anything fun today?' },
        { speaker: 'You', text: 'I walked in the park and got some coffee.' },
      ],
      quickReplies: ["I'm doing great, thanks!", 'Today was busy, honestly.', 'I have a fun weekend plan.'],
    },
    'B1-B2': {
      summary: 'Natural catch-up with follow-up details and reactions.',
      lines: [
        { speaker: 'Tutor', text: "Hey! How's your week been treating you?" },
        { speaker: 'You', text: "Honestly, it's been pretty busy, though I'm hanging in there." },
        { speaker: 'Tutor', text: 'I feel you. Anything fun lined up once things slow down?' },
        { speaker: 'You', text: "Yeah, I'm hoping to grab dinner with a few friends on Friday." },
      ],
      quickReplies: ["It's been hectic, to be honest.", 'Actually, I just started a new hobby!', 'How about you — anything exciting this week?'],
    },
    'C1-C2': {
      summary: 'Idiomatic, relaxed small talk with nuanced transitions.',
      lines: [
        { speaker: 'Tutor', text: "Hey! Sounds like you've got a lot on your plate — what's been top of mind?" },
        { speaker: 'You', text: "A bit of everything, honestly, though I've been making time to unwind lately." },
        { speaker: 'Tutor', text: "Smart move. What's helped you disconnect most?" },
        { speaker: 'You', text: 'Getting outdoors without checking my phone has made a huge difference.' },
      ],
      quickReplies: ['Trying to strike a better work-life balance lately.', 'Funny you ask — I was just mulling that over.', 'That reminds me of something I read yesterday.'],
    },
  },
  cafe_order: {
    'A1-A2': {
      summary: 'Short polite phrases for ordering a drink and paying.',
      lines: [
        { speaker: 'Tutor', text: 'Hi! What can I get you today?' },
        { speaker: 'You', text: 'Hi! Can I get a medium latte, please?' },
        { speaker: 'Tutor', text: 'Sure! Hot or iced?' },
        { speaker: 'You', text: 'Iced, please. And one croissant.' },
      ],
      quickReplies: ['Can I get a medium iced latte, please?', 'Hot is fine, thank you.', 'How much is it?'],
    },
    'B1-B2': {
      summary: 'Customize your coffee order smoothly with oat milk and pastries.',
      lines: [
        { speaker: 'Tutor', text: 'Morning! What can I get started for you?' },
        { speaker: 'You', text: 'Could I grab a medium iced latte with oat milk, please?' },
        { speaker: 'Tutor', text: 'Of course! Anything else for you today?' },
        { speaker: 'You', text: "Actually, could you warm up an almond croissant to go?" },
      ],
      quickReplies: ['Could I get that with oat milk instead?', 'Any chance that comes decaf?', "That's all for now — thanks!"],
    },
    'C1-C2': {
      summary: 'Casual native coffee-shop banter and quick add-ons.',
      lines: [
        { speaker: 'Tutor', text: 'Morning! Usual or switching it up today?' },
        { speaker: 'You', text: "Let's mix it up — I'll do an iced oat flat white if you guys still have the single-origin beans." },
        { speaker: 'Tutor', text: 'We sure do! For here or to go?' },
        { speaker: 'You', text: "To go, please — and I'll tack on one of those banana loaves if they're fresh." },
      ],
      quickReplies: ["I'll go with the seasonal roast if it's light.", 'Any chance you could go easy on the ice?', 'Mind leaving room for a splash of cream?'],
    },
  },
  job_interview: {
    'A1-A2': {
      summary: 'Clear, simple answers about your job and strengths.',
      lines: [
        { speaker: 'Tutor', text: 'Nice to meet you! Can you tell me about yourself?' },
        { speaker: 'You', text: 'Thanks! I am a web developer. I have three years of experience.' },
        { speaker: 'Tutor', text: 'Great! What are your strong skills?' },
        { speaker: 'You', text: 'I learn fast and enjoy working with a team.' },
      ],
      quickReplies: ['I worked as a developer for two years.', 'I like solving problems with my team.', 'When does the team usually meet?'],
    },
    'B1-B2': {
      summary: 'Share achievements concisely and highlight teamwork impact.',
      lines: [
        { speaker: 'Tutor', text: 'Thanks for joining today! Could you walk me through your background?' },
        { speaker: 'You', text: "Sure! Over the last three years I've built customer apps and helped cut page load time in half." },
        { speaker: 'Tutor', text: 'Impressive. What challenge are you most proud of solving?' },
        { speaker: 'You', text: 'When a launch slipped, I coordinated with QA to fix the bottleneck before release.' },
      ],
      quickReplies: ["In my last role, I led a feature from design to launch.", 'I thrive when collaborating closely with product teams.', "I'd love to hear more about your engineering culture."],
    },
    'C1-C2': {
      summary: 'Articulate impact, trade-offs, and strategic alignment smoothly.',
      lines: [
        { speaker: 'Tutor', text: 'To kick things off, what drew you to this role right now?' },
        { speaker: 'You', text: "I'm looking for a place where product ownership and technical depth intersect, and your roadmap really stood out." },
        { speaker: 'Tutor', text: 'How do you approach tough trade-offs under tight deadlines?' },
        { speaker: 'You', text: 'I align early on what is truly critical for users, ship cleanly, and pay down debt intentionally.' },
      ],
      quickReplies: ['The biggest impact came from simplifying our core workflow.', 'I weigh speed against maintainability before committing.', 'What does success look like in the first 90 days?'],
    },
  },
  polite_decline: {
    'A1-A2': {
      summary: 'Thank the person warmly and give a short, kind excuse.',
      lines: [
        { speaker: 'Tutor', text: "Hey! I'm having dinner this Sunday. Can you come?" },
        { speaker: 'You', text: "Thanks for inviting me! I'm sorry, I can't make it Sunday." },
        { speaker: 'Tutor', text: 'Oh, no worries! Are you busy?' },
        { speaker: 'You', text: 'Yes, I have family plans. Maybe next weekend?' },
      ],
      quickReplies: ["Thanks for asking, but I'm busy Sunday.", "Sorry, I can't make it this time!", 'Can we hang out next week instead?'],
    },
    'B1-B2': {
      summary: 'Soften a refusal with appreciation and suggest another time.',
      lines: [
        { speaker: 'Tutor', text: "I'm hosting a late dinner party this Sunday night. Can you make it?" },
        { speaker: 'You', text: "That sounds awesome, thanks so much for inviting me! Unfortunately I'm tied up Sunday evening." },
        { speaker: 'Tutor', text: 'Totally understand — maybe another time?' },
        { speaker: 'You', text: "Definitely! Let's grab coffee next week so we can catch up." },
      ],
      quickReplies: ["I'd love to, though I'm out of town this Sunday.", 'I really appreciate the invite, just need an early night.', 'Any chance we could rain-check for next weekend?'],
    },
    'C1-C2': {
      summary: 'Graciously decline with tact and zero awkwardness.',
      lines: [
        { speaker: 'Tutor', text: 'I am hosting a late dinner party this Sunday — any chance you can swing by?' },
        { speaker: 'You', text: "So kind of you to think of me! Sadly I've already got something on the calendar that night." },
        { speaker: 'Tutor', text: 'All good, we will miss you there!' },
        { speaker: 'You', text: "Count me in for the next one — let's put something on the books soon." },
      ],
      quickReplies: ["Wish I could swing it, I'm completely swamped Sunday.", "Thanks for thinking of me — I'll have to take a rain check.", "Save me a seat next time, I'd love to catch up soon."],
    },
  },
};
