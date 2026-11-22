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
  cafe_order: { spokenReply: 'Good morning! Welcome to Sunrise Cafe. What can I get started for you today?', followUpPrompt: 'What drink or size would you like to order?' },
  job_interview: { spokenReply: 'Hello and welcome! Thank you for joining this interview today. Could you start by introducing yourself and your background?', followUpPrompt: 'How would you introduce your recent experience?' },
  polite_decline: { spokenReply: 'Hey! I am hosting a late dinner party this Sunday night and would love for you to come. Can you make it?', followUpPrompt: 'How can you thank them and politely decline?' },
  hotel_checkin: { spokenReply: 'Good evening, welcome to the Grand Harbor Hotel! How may I help you check in today?', followUpPrompt: 'Could you share your reservation details?' },
  restaurant_dining: { spokenReply: "Welcome to The Olive Table! Here is your menu. Would you like to hear today's specials before you order?", followUpPrompt: 'What would you like to ask the server first?' },
  doctor_pharmacy: { spokenReply: 'Hello! Welcome to the clinic pharmacy. Tell me how you are feeling today and how I can help.', followUpPrompt: 'What symptoms have you been experiencing?' },
  team_standup: { spokenReply: "Good morning! Let's kick off our daily standup. How did your work go yesterday, and what is your main priority today?", followUpPrompt: 'What progress or blocker would you like to share?' },
};

export function findFallbackScenario(id?: string): RoleplayScenario {
  const target = (id ?? '').trim().toLowerCase();
  return DEFAULT_ROLEPLAY_SCENARIOS.find((s) => s.id.toLowerCase() === target) ?? DEFAULT_ROLEPLAY_SCENARIOS[0];
}

export function buildOfflineScenarioOpener(scenarioId?: string, topic?: string): TutorReply {
  if (scenarioId && SCENARIO_OPENERS[scenarioId]) return SCENARIO_OPENERS[scenarioId];
  const cleanTopic = (topic ?? '').trim();
  const hasCustomTopic = cleanTopic.length > 0 && cleanTopic.toLowerCase() !== 'everyday conversation' && cleanTopic.toLowerCase() !== 'free conversation';
  return {
    spokenReply: hasCustomTopic
      ? `Hi there! Let's practice speaking English about ${cleanTopic}. What comes to mind first?`
      : 'Hi there! I am your English speaking tutor. How is your day going, and what would you like to talk about?',
    followUpPrompt: 'How is your day going so far?',
  };
}
