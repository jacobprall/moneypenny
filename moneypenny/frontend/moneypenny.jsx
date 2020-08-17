import ReactDOM from "react-dom";
import configureStore from "./store/store";
import Root from "./components/root";
import React from 'react'
import { signOut } from './util/session_api_util'

document.addEventListener('DOMContentLoaded', () => {
  let store;
  if (window.currentUser) {
    const { currentUser } = window;
    const { id } = currentUser;
    const preloadedState = {
      entities: {
        users: {
          [id]: currentUser
        }
      },
    session: { id }
    };
    store = configureStore(preloadedState);

    delete window.currentUser;

  } else {
    store = configureStore();
  }
  window.getState = store.getState;
  window.logout = signOut
  const root = document.getElementById('root');
  ReactDOM.render(<Root store={store} />, root);
});
  
  

