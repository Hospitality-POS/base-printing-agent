/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        green: {
          400: "#00E700",
          500: "#00E700",
          600: "#00cc00",
          700: "#009900",
          900: "#003a00",
        },
      },
    },
  },
  plugins: [],
};
