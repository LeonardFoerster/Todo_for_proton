import { mount } from "svelte";
import "../src/tokens/tokens.css";
import Demo from "./Demo.svelte";

mount(Demo, { target: document.getElementById("app")! });
