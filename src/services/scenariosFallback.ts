import type { CefrLevel, CommunicativeGoal, RoleplayScenario, TutorReply } from '../types/domain';

const goal = (id: string, description: string, kw: string): CommunicativeGoal => ({
  id,
  description,
  keywords: kw.split(',').map((s) => s.trim()).filter(Boolean),
});

const scenario = (
  id: string,
  title: string,
  summary: string,
  tutorPersona: string,
  learnerRole: string,
  defaultCefr: CefrLevel,
  vocab: string,
  goals: CommunicativeGoal[],
): RoleplayScenario => ({
  id, title, summary, tutorPersona, learnerRole, defaultCefr, defaultMaxTurns: 6,
  targetVocabulary: vocab ? vocab.split(',').map((s) => s.trim()).filter(Boolean) : [],
  goals,
});

/**
 * Pure-English 8-scenario offline catalog mirroring `src-tauri/src/domain/scenario_presets*.rs`
 * when no local Rust bridge is reachable (e.g. Cloudflare Workers static hosting).
 */
export const DEFAULT_ROLEPLAY_SCENARIOS: RoleplayScenario[] = [
  scenario('free_conversation', 'Everyday Conversation', 'Natural warm chat about your day, interests, and routines.',
    'Warm, patient conversational English partner', 'Friendly speaker practicing everyday topics', 'B1', '',
    [goal('share_update', 'Share what happened today or a personal plan', 'today,yesterday,tomorrow,plan,work,weekend'),
      goal('ask_partner', 'Ask your partner a natural follow-up question', 'how about you,what about you,do you,have you,why')]),
  scenario('cafe_order', 'Ordering at a Coffee Shop', 'Order a customized drink, ask about snacks, and pay politely.',
    'Friendly specialty cafe barista', 'Customer ordering drinks and pastries', 'A2', 'espresso,oat milk,for here,to go,receipt,pastry',
    [goal('order_drink', 'Order a drink with size or milk preference', 'latte,coffee,tea,espresso,oat milk,cup,size,iced'),
      goal('ask_option', 'Ask about ingredients, flavors, or food items', 'pastry,muffin,croissant,gluten,recommend,sugar,have'),
      goal('checkout_choice', 'Say for here or to go and complete checkout', 'for here,to go,takeaway,card,cash,receipt,pay')]),
  scenario('job_interview', 'Job Interview Practice', 'Present strengths, explain a challenge, and ask thoughtful questions.',
    'Constructive hiring manager running a behavioral interview', 'Candidate interviewing for a collaborative role', 'B2', 'collaborate,deadline,challenge,impact,strength,feedback',
    [goal('intro_strength', 'Introduce your background and one strength', 'experience,strength,background,years,worked,good at'),
      goal('past_challenge', 'Describe how you handled a challenge or deadline', 'challenge,problem,deadline,solved,team,result'),
      goal('ask_role', 'Ask the interviewer a question about the team or role', 'team,culture,day-to-day,expectation,opportunity,role')]),
  scenario('polite_decline', 'Declining Politely', 'Say no diplomatically while keeping the relationship warm.',
    'Warm colleague inviting you to an extra commitment', 'Colleague declining graciously with an alternative', 'B1', 'appreciate,schedule,unfortunately,another time,rain check,grateful',
    [goal('thank_warmly', 'Thank them sincerely before declining', 'thank,appreciate,grateful,kind'),
      goal('give_reason', 'Give a concise, polite reason', 'unfortunately,schedule,busy,already,commitment,conflict'),
      goal('offer_alternative', 'Suggest an alternative time or future option', 'next week,another time,rain check,instead,later,tomorrow')]),
  scenario('hotel_checkin', 'Hotel Check-In', 'Confirm your reservation, ask about amenities, and make a request.',
    'Welcoming front-desk hotel receptionist', 'Arriving guest checking into a hotel', 'A2', 'reservation,confirmation,passport,check-out,breakfast,key card',
    [goal('confirm_booking', 'State your reservation and check-in details', 'reservation,booking,booked,name,confirmation,nights'),
      goal('ask_amenity', 'Ask about breakfast, Wi-Fi, or check-out hours', 'breakfast,wifi,wi-fi,check-out,checkout,included'),
      goal('polite_request', 'Make a polite room preference or request', 'quiet,floor,pillow,towel,view,late checkout,could i')]),
  scenario('restaurant_dining', 'Restaurant Dining', 'Order a meal in English, ask about ingredients, and request the bill.',
    'Friendly restaurant server taking dinner orders in English', 'Diner ordering a main course and asking about menu options', 'A2', 'recommend,special,side dish,allergic,bill',
    [goal('ask_recommendation', 'Ask the server for a recommendation or daily special', 'recommend,special,popular,suggest'),
      goal('order_main_and_side', 'Order a main dish with a side or dietary preference', 'order,side,salad,allergic,chicken,pasta,steak'),
      goal('request_bill', 'Ask for the bill or payment method', 'bill,check,card,pay')]),
  scenario('doctor_pharmacy', 'Doctor or Pharmacy Visit', 'Describe symptoms clearly in English and confirm medicine dosage.',
    'Caring clinic pharmacist helping a patient in English', 'Patient explaining symptoms and asking how to take medication', 'B1', 'symptoms,dosage,twice a day,side effects,relief',
    [goal('describe_symptoms', 'Describe your symptoms and how long you have had them', 'symptom,headache,throat,cough,fever,since,days'),
      goal('confirm_dosage', 'Ask how often or when to take the medicine', 'dosage,often,twice,meal,take,hours'),
      goal('check_side_effects', 'Ask about possible side effects or precautions', 'side effect,drowsy,avoid,safe')]),
  scenario('team_standup', 'Team Standup & Workplace Small Talk', 'Share work progress, flag a blocker, and align on next steps in English.',
    'Supportive engineering lead running a daily standup in English', "Teammate sharing yesterday's progress, today's priority, and a blocker", 'B2', 'progress,blocker,priority,follow up,deadline',
    [goal('share_progress', 'Summarize what you completed recently', 'completed,finished,progress,worked on,shipped'),
      goal('flag_blocker', 'Explain a current blocker or dependency clearly', 'blocker,blocked,waiting,review,issue'),
      goal('align_next_priority', 'State your next priority or follow-up plan', 'priority,next,follow up,deadline,focus')]),
];

const SCENARIO_OPENERS: Record<string, TutorReply> = {
  cafe_order: {
    spokenReply: 'Morning! Welcome to Sunrise Cafe. What can I get started for you?',
    followUpPrompt: 'What drink or size sounds good today?',
    suggestedReplies: ['Could I get a medium iced latte, please?', "I'll have a hot black coffee.", 'What pastries do you have today?'],
  },
  job_interview: {
    spokenReply: "Hi, thanks so much for hopping on today! Could you start by telling me a bit about yourself?",
    followUpPrompt: 'What kind of work have you been focused on lately?',
    suggestedReplies: ['Sure! I have two years of software experience.', "I'm looking for a role where I can grow my skills.", 'Lately I worked on a customer portal project.'],
  },
  polite_decline: {
    spokenReply: "Hey! I'm throwing a dinner party this Sunday night and would love for you to come. Can you make it?",
    followUpPrompt: 'How can you thank them and turn it down nicely?',
    suggestedReplies: ["Thanks so much for inviting me, but I'm busy Sunday.", "That sounds fun! Sadly I already have plans.", "Wish I could make it — maybe next weekend?"],
  },
  hotel_checkin: {
    spokenReply: 'Good evening, welcome to the Grand Harbor Hotel! Checking in tonight?',
    followUpPrompt: 'Could you share the name on your booking?',
    suggestedReplies: ['Hi! Yes, I have a reservation under my name.', "I'm checking in for two nights.", 'Could I get a quiet room if possible?'],
  },
  restaurant_dining: {
    spokenReply: "Welcome to The Olive Table! Here's your menu. Can I get you started with water or today's specials?",
    followUpPrompt: 'What would you like to ask your server first?',
    suggestedReplies: ["I'd love to hear today's specials!", 'Could we start with two glasses of water?', 'Does the soup have any nuts or dairy?'],
  },
  doctor_pharmacy: {
    spokenReply: 'Hi there, welcome to the pharmacy. What brings you in today — how are you feeling?',
    followUpPrompt: 'What symptoms have you been having?',
    suggestedReplies: ["I've had a sore throat since yesterday.", 'Do you have something mild for a headache?', 'How many times a day should I take this?'],
  },
  team_standup: {
    spokenReply: "Morning! Let's jump into standup. How did yesterday go, and what's your priority today?",
    followUpPrompt: 'Any progress or blockers to share?',
    suggestedReplies: ['Yesterday I fixed the login bug. Today I am testing.', "I'm waiting on a code review for my PR.", 'Everything is on track for Friday!'],
  },
};

const BASIC_SCENARIO_OPENERS: Record<string, TutorReply> = {
  cafe_order: { spokenReply: 'Hi! Welcome to Sunrise Cafe. What can I get you today?', followUpPrompt: 'Coffee, tea, or food?', suggestedReplies: ['Hi! Can I get a coffee, please?', 'One iced tea, please.', 'Do you have any cake?'] },
  job_interview: { spokenReply: 'Hi, nice to meet you! Can you tell me a little about you?', followUpPrompt: 'Where do you work or study?', suggestedReplies: ['Hi! Nice to meet you too.', 'I work in tech support.', 'I am good at helping people.'] },
  polite_decline: { spokenReply: "Hey! I'm having dinner this Sunday. Can you come?", followUpPrompt: 'How can you say no nicely?', suggestedReplies: ["Thanks, but I'm busy Sunday.", "Sorry, I can't come this time.", 'Maybe next week!'] },
  hotel_checkin: { spokenReply: 'Hi! Welcome to our hotel. Checking in?', followUpPrompt: "What's your name, please?", suggestedReplies: ['Yes, I have a booking.', 'My name is Alex.', 'Is breakfast free?'] },
  restaurant_dining: { spokenReply: "Hi! Here's your menu. Would you like some water first?", followUpPrompt: 'What do you want to eat or drink?', suggestedReplies: ['Yes, water please.', 'Can I see the specials?', 'I want the grilled chicken, please.'] },
  doctor_pharmacy: { spokenReply: 'Hi! How do you feel today?', followUpPrompt: 'What hurts right now?', suggestedReplies: ['I have a headache.', 'I have a cough since Monday.', 'Can you give me medicine for a cold?'] },
  team_standup: { spokenReply: "Morning! What did you do yesterday, and what's today's plan?", followUpPrompt: 'What are you working on?', suggestedReplies: ['Yesterday I finished my page.', 'Today I will test the app.', 'I need a little help with one bug.'] },
};

export function findFallbackScenario(id?: string): RoleplayScenario {
  const target = (id ?? '').trim().toLowerCase();
  return DEFAULT_ROLEPLAY_SCENARIOS.find((s) => s.id.toLowerCase() === target) ?? DEFAULT_ROLEPLAY_SCENARIOS[0];
}

export function buildOfflineScenarioOpener(scenarioId?: string, topic?: string, cefrLevel?: CefrLevel): TutorReply {
  const basic = cefrLevel === 'A1' || cefrLevel === 'A2';
  if (scenarioId && basic && BASIC_SCENARIO_OPENERS[scenarioId]) return BASIC_SCENARIO_OPENERS[scenarioId];
  if (scenarioId && SCENARIO_OPENERS[scenarioId]) return SCENARIO_OPENERS[scenarioId];
  const cleanTopic = (topic ?? '').trim();
  const hasCustomTopic = cleanTopic.length > 0 && cleanTopic.toLowerCase() !== 'everyday conversation' && cleanTopic.toLowerCase() !== 'free conversation';
  return {
    spokenReply: hasCustomTopic
      ? `Hey! Let's chat about ${cleanTopic}. What comes to mind first?`
      : basic ? "Hi! Nice to meet you. How's your day going?" : "Hey there! Glad we're chatting today. How's your day going so far?",
    followUpPrompt: basic ? 'How are you today?' : "What's been the best part of your day?",
    suggestedReplies: basic
      ? ["I'm good, thanks! How about you?", 'Today was pretty good.', 'Let us talk about my hobby.']
      : ['Pretty good so far! I grabbed coffee earlier.', "It's been busy, actually — I'm glad to take a break.", 'Let us talk about weekend plans!'],
  };
}
