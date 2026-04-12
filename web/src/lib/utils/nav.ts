export type NavIconKey = 'schedule' | 'analytics' | 'search' | 'bar_chart' | 'settings';

export type NavItem = {
  href: string;
  label: string;
  icon: NavIconKey;
};

export const navItems: NavItem[] = [
  {
    href: '/',
    label: 'Timeline',
    icon: 'schedule',
  },
  {
    href: '/insights',
    label: 'Insights',
    icon: 'analytics',
  },
  {
    href: '/search',
    label: 'Search',
    icon: 'search',
  },
  {
    href: '/stats',
    label: 'Stats',
    icon: 'bar_chart',
  },
];
