/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      keyframes: {
        "tray-popover-in": {
          "0%": { opacity: "0", transform: "translateY(8px) scale(0.96)" },
          "100%": { opacity: "1", transform: "translateY(0) scale(1)" }
        },
        "battery-wave": {
          "0%, 100%": { opacity: "0.85" },
          "50%": { opacity: "1.0" }
        }
      },
      animation: {
        "tray-popover-in": "tray-popover-in 180ms cubic-bezier(0.22, 1, 0.36, 1) forwards",
        "battery-wave": "battery-wave 1.8s ease-in-out infinite"
      }
    },
  },
  plugins: [],
};
