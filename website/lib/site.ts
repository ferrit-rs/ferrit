const CREATOR_LINKS = [
  { label: "GitHub", href: "https://github.com/richardlavoura" },
  { label: "LinkedIn", href: "https://www.linkedin.com/in/richardlavoura" },
  { label: "Website", href: "https://richardlavoura.dev" },
];

export const siteConfig = {
  name: "Ferrit",
  description:
    "Ferrit is the everyday Git manager for your terminal, built in Rust by Richard Lavoura.",
  url: "https://github.com/ferrit-rs/ferrit",
  ogImage: "/og-image.webp",
  about: {
    stats: [
      { value: "8 yrs", label: "Data and analytics engineering" },
      { value: "Rust", label: "Systems and backend direction" },
      { value: "v0.10", label: "Ferrit public release" },
    ],
  },
  contact: {
    email: "richard.lavoura@gmail.com",
  },
  links: {
    github: "https://github.com/ferrit-rs/ferrit",
    personalWebsite: "https://richardlavoura.dev",
  },
  author: {
    name: "Richard Lavoura",
    url: "https://www.linkedin.com/in/richardlavoura",
    image: "/images/richard/portrait.webp",
  },
  creator: {
    name: "Richard Lavoura",
    role: "Data and systems engineer",
    profileUrl: "https://www.linkedin.com/in/richardlavoura",
    links: CREATOR_LINKS,
  },
  footer: {
    blurb:
      "Ferrit is a public Rust project for making everyday Git work visible, focused, and dependable.",
    groups: [
      {
        heading: "Explore",
        links: [
          { label: "Project overview", href: "/project" },
          { label: "Articles", href: "/articles" },
          { label: "My story", href: "/about" },
        ],
      },
      {
        heading: "Connect",
        links: [
          { label: "Contact", href: "/contact" },
          { label: "GitHub", href: "https://github.com/richardlavoura" },
          { label: "LinkedIn", href: "https://www.linkedin.com/in/richardlavoura" },
        ],
      },
      {
        heading: "Project",
        links: [
          { label: "Ferrit overview", href: "/project" },
          { label: "Design choices", href: "/design" },
          { label: "Richard's work", href: "/work" },
        ],
      },
    ],
  },
};
