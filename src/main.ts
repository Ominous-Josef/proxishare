import { createApp } from "vue";
import App from "./App.vue";

const app = createApp(App);

app.config.errorHandler = (err, _instance, info) => {
  console.error("Vue Error:", err, info);
  alert(
    `An unexpected error occurred: ${err}\n\nPlease report this in the Support tab.`
  );
};

app.mount("#app");
