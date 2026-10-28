/**
 * Local assistant categories. Each sidebar item is a query over mail already
 * on disk: thread subject, sender domain, and the List-Unsubscribe header.
 * Nothing is uploaded for classification.
 *
 * A thread can match more than one leaf rule (a Netflix receipt is both
 * "entertainment" and "bills", and almost every commercial message carries
 * List-Unsubscribe). Views are a priority partition, most specific first, so
 * each thread lands in one sidebar item.
 */

export interface AssistantCategory {
  id: string;
  /** English label. Callers pass it through localized(). */
  label: string;
  iconName: string;
  /** Case-insensitive substrings of the thread subject. */
  subjects: string[];
  /** Sender domains, including their subdomains. */
  domains: string[];
  /** Any message on the thread carries a List-Unsubscribe header. */
  listUnsubscribe?: boolean;
}

/** Sidebar order. Classification priority is the order of ASSISTANT_CATEGORIES. */
export const ASSISTANT_SIDEBAR_ORDER = [
  'subscriptions',
  'travel',
  'packages',
  'bills',
  'entertainment',
];

export const ASSISTANT_CATEGORIES: AssistantCategory[] = [
  {
    id: 'packages',
    label: 'Packages',
    iconName: 'archive.png',
    subjects: ['shipped', 'out for delivery', 'tracking number', 'your package', 'shipment'],
    domains: ['ups.com', 'fedex.com', 'usps.com', 'dhl.com'],
  },
  {
    id: 'travel',
    label: 'Travel',
    iconName: 'sent.png',
    subjects: [
      'boarding pass',
      'itinerary',
      'flight confirmation',
      'e-ticket',
      'gate change',
      'hotel confirmation',
    ],
    domains: [
      'united.com',
      'delta.com',
      'aa.com',
      'southwest.com',
      'jetblue.com',
      'alaskaair.com',
      'lufthansa.com',
      'airfrance.com',
      'britishairways.com',
      'airbnb.com',
      'booking.com',
      'expedia.com',
      'hotels.com',
      'marriott.com',
      'hilton.com',
      'hyatt.com',
      'vrbo.com',
    ],
  },
  {
    id: 'entertainment',
    label: 'Entertainment',
    iconName: 'starred.png',
    subjects: ['now streaming', 'new episode'],
    domains: [
      'netflix.com',
      'spotify.com',
      'hulu.com',
      'disneyplus.com',
      'primevideo.com',
      'ticketmaster.com',
      'eventbrite.com',
      'steampowered.com',
      'twitch.tv',
    ],
  },
  {
    id: 'bills',
    label: 'Bill & Receipts',
    iconName: 'tag.png',
    subjects: [
      'receipt',
      'invoice',
      'billing statement',
      'payment received',
      'your bill',
      'amount due',
    ],
    domains: ['paypal.com', 'stripe.com', 'squareup.com', 'intuit.com'],
  },
  {
    id: 'subscriptions',
    label: 'Subscriptions',
    iconName: 'activity.png',
    subjects: ['unsubscribe', 'newsletter'],
    domains: ['mailchimp.com', 'list-manage.com', 'substack.com', 'beehiiv.com'],
    listUnsubscribe: true,
  },
];

export function assistantCategoryById(id: string) {
  return ASSISTANT_CATEGORIES.find((category) => category.id === id) || null;
}

/** Leaf match for one category, before higher-priority categories take the thread. */
export function leafFilterForAssistantCategory(id: string) {
  const category = assistantCategoryById(id);
  if (!category) {
    return null;
  }
  const or: object[] = [];
  if (category.listUnsubscribe) {
    or.push({ listUnsubscribe: true });
  }
  for (const subject of category.subjects) {
    or.push({ subject });
  }
  if (category.domains.length > 0) {
    or.push({ from: category.domains });
  }
  return { or };
}

/**
 * The view filter. Categories earlier in ASSISTANT_CATEGORIES win, so a
 * shipped receipt stays in Packages and a mailing list stays in Subscriptions.
 */
export function filterForAssistantCategory(id: string) {
  const own = leafFilterForAssistantCategory(id);
  if (!own) {
    return null;
  }
  const index = ASSISTANT_CATEGORIES.findIndex((category) => category.id === id);
  const higher = ASSISTANT_CATEGORIES.slice(0, index)
    .map((category) => leafFilterForAssistantCategory(category.id))
    .filter(Boolean);
  if (higher.length === 0) {
    return own;
  }
  return { and: [own, ...higher.map((other) => ({ not: other }))] };
}
