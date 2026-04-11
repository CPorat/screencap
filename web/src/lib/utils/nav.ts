export type NavIconKey = 'timeline' | 'insights' | 'search' | 'stats';

export type NavItem = {
  href: string;
  label: string;
  caption: string;
  icon: NavIconKey;
};

export const navItems: NavItem[] = [
  {
    href: '/',
    label: 'Timeline',
    caption: 'Live activity ribbon',
    icon: 'timeline',
  },
  {
    href: '/insights',
    label: 'Insights',
    caption: 'Summaries and focus',
    icon: 'insights',
  },
  {
    href: '/search',
    label: 'Search',
    caption: 'Retrieve exact moments',
    icon: 'search',
  },
  {
    href: '/stats',
    label: 'Stats',
    caption: 'System telemetry',
    icon: 'stats',
  },
];
