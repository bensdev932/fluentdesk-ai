import type { PreConversationScript } from './scriptsDataCore';
import type { ScriptTier } from './preConversationScripts';

export const EXTRA_SCRIPTS: Record<string, Record<ScriptTier, PreConversationScript>> = {
  hotel_checkin: {
    'A1-A2': {
      summary: 'Check in at the front desk and ask about breakfast.',
      lines: [
        { speaker: 'Tutor', text: 'Hi! Welcome to our hotel. Checking in?' },
        { speaker: 'You', text: 'Yes! I have a booking under Alex Chen.' },
        { speaker: 'Tutor', text: 'Found it! Two nights, right?' },
        { speaker: 'You', text: 'Yes, two nights. What time is breakfast?' },
      ],
      quickReplies: ['Yes, I have a reservation under my name.', 'Could I have a quiet room, please?', 'Is Wi-Fi free in the room?'],
    },
    'B1-B2': {
      summary: 'Confirm booking details, room preferences, and late checkout.',
      lines: [
        { speaker: 'Tutor', text: 'Good evening, welcome to the Grand Harbor Hotel! Checking in?' },
        { speaker: 'You', text: "Hi there! Yes, I have a reservation for two nights under Alex." },
        { speaker: 'Tutor', text: 'Got you right here. Any preferences for your stay?' },
        { speaker: 'You', text: 'If possible, could I get a quiet room on a higher floor?' },
      ],
      quickReplies: ['Any chance of a late checkout on Sunday?', 'Could you let me know the Wi-Fi password?', 'Where are the elevators from here?'],
    },
    'C1-C2': {
      summary: 'Handle room requests and concierge questions effortlessly.',
      lines: [
        { speaker: 'Tutor', text: 'Welcome in! Are you checking in with us tonight?' },
        { speaker: 'You', text: "I am, thanks — should be under Alex for two nights." },
        { speaker: 'Tutor', text: 'All set. Would you like a quiet room overlooking the courtyard?' },
        { speaker: 'You', text: "That'd be wonderful, thank you. Also, is late checkout an option if flights run late?" },
      ],
      quickReplies: ['If you have a higher floor open, that would be ideal.', 'Could you recommend a good spot for dinner nearby?', "I'll settle incidentals on this card, please."],
    },
  },
  restaurant_dining: {
    'A1-A2': {
      summary: 'Ask about specials, order a meal, and request the check.',
      lines: [
        { speaker: 'Tutor', text: "Hi! Here's your menu. Would you like water first?" },
        { speaker: 'You', text: 'Yes, still water please. What are the specials?' },
        { speaker: 'Tutor', text: 'Today we have grilled salmon and fresh pasta.' },
        { speaker: 'You', text: "I'll have the grilled salmon, please." },
      ],
      quickReplies: ["What do you recommend tonight?", 'Does this dish have peanuts?', 'Could I get the bill, please?'],
    },
    'B1-B2': {
      summary: 'Check ingredients for allergies and order naturally.',
      lines: [
        { speaker: 'Tutor', text: "Welcome! Can I start you with drinks or today's specials?" },
        { speaker: 'You', text: "I'd love to hear today's specials, and could we get sparkling water?" },
        { speaker: 'Tutor', text: 'Sure! Our chef made roasted herb chicken and mushroom risotto.' },
        { speaker: 'You', text: 'Both sound amazing — does the risotto have any tree nuts?' },
      ],
      quickReplies: ["I'll go with the pasta, please.", 'Could we get the sauce on the side?', 'Whenever you have a second, could we grab the check?'],
    },
    'C1-C2': {
      summary: 'Dine naturally with pairing questions and dietary nuances.',
      lines: [
        { speaker: 'Tutor', text: 'Welcome to The Olive Table! Ready for drinks or specials?' },
        { speaker: 'You', text: "Walk us through the specials first, and we'll start with a carafe of sparkling." },
        { speaker: 'Tutor', text: "Tonight's standout is pan-seared sea bass with lemon caper butter." },
        { speaker: 'You', text: "Sold — let's do that, and mind keeping it gluten-free if possible?" },
      ],
      quickReplies: ['Which starter pairs best with that main?', "We're in no rush — take your time.", 'Could we split the check evenly when you get a chance?'],
    },
  },
  doctor_pharmacy: {
    'A1-A2': {
      summary: 'Explain symptoms clearly and check how often to take medicine.',
      lines: [
        { speaker: 'Tutor', text: 'Hi! How are you feeling today?' },
        { speaker: 'You', text: 'Not great. I have a sore throat and a headache.' },
        { speaker: 'Tutor', text: 'How long have you had that?' },
        { speaker: 'You', text: 'Since two days ago. How often do I take this?' },
      ],
      quickReplies: ['I have had a fever since last night.', 'Should I take this with food?', 'Will this medicine make me sleepy?'],
    },
    'B1-B2': {
      summary: 'Describe symptom timeline, check dosage, and ask about side effects.',
      lines: [
        { speaker: 'Tutor', text: 'Welcome to the pharmacy! What brings you in today?' },
        { speaker: 'You', text: "I've had a dry cough and mild fever since Tuesday morning." },
        { speaker: 'Tutor', text: 'This syrup will help ease the cough quickly.' },
        { speaker: 'You', text: 'Great — should I take it twice a day after meals, and does it cause drowsiness?' },
      ],
      quickReplies: ["I've had these symptoms for about three days.", 'Are there any side effects I should watch for?', 'Is it okay to take this with painkillers?'],
    },
    'C1-C2': {
      summary: 'Discuss symptoms, interactions, and precautions accurately.',
      lines: [
        { speaker: 'Tutor', text: 'Hi there — how have you been feeling, and what can I help with?' },
        { speaker: 'You', text: "I came down with a nagging cough and scratchy throat two days ago; it's mostly flaring up at night." },
        { speaker: 'Tutor', text: "Here's an over-the-counter option that targets nighttime coughing." },
        { speaker: 'You', text: 'Any interactions I should be mindful of, or does it make you drowsy during the day?' },
      ],
      quickReplies: ['It started gradually on Monday evening.', 'What is the maximum daily dose here?', 'Should I follow up with a doctor if it lingers?'],
    },
  },
  team_standup: {
    'A1-A2': {
      summary: 'Share yesterday, today, and one simple blocker.',
      lines: [
        { speaker: 'Tutor', text: "Morning! What did you work on yesterday?" },
        { speaker: 'You', text: 'Yesterday I finished the login form.' },
        { speaker: 'Tutor', text: 'Nice! What is your plan today?' },
        { speaker: 'You', text: "Today I'm testing the page, and I need help with one bug." },
      ],
      quickReplies: ['Yesterday I fixed two small bugs.', "Today I'm working on the search page.", 'I am waiting for feedback from Sam.'],
    },
    'B1-B2': {
      summary: 'Give crisp standup updates on progress, blockers, and follow-ups.',
      lines: [
        { speaker: 'Tutor', text: "Morning! How did yesterday go, and what's your main priority today?" },
        { speaker: 'You', text: "Yesterday I wrapped up the payment API, and today I'm writing integration tests." },
        { speaker: 'Tutor', text: 'Sounds solid. Running into any blockers?' },
        { speaker: 'You', text: "Just one — I'm waiting on staging credentials before Friday's deadline." },
      ],
      quickReplies: ['Everything is on track for this sprint.', "I hit a minor blocker with the test database.", "Let's sync for five minutes right after standup."],
    },
    'C1-C2': {
      summary: 'Concise engineering standup with clear alignment and risk callouts.',
      lines: [
        { speaker: 'Tutor', text: "Morning team! What's moving forward and what needs eyes today?" },
        { speaker: 'You', text: "I landed the caching refactor yesterday; today I'm benchmarking latency before cutover." },
        { speaker: 'Tutor', text: 'Any dependencies putting the deadline at risk?' },
        { speaker: 'You', text: "One dependency on schema sign-off — I'll follow up with Infra right after this call." },
      ],
      quickReplies: ['All green on my end — shipping to staging this afternoon.', 'I could use a quick sanity check on the migration PR.', 'Happy to pair on that flaky test once we wrap up.'],
    },
  },
};
