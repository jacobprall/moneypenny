import { RECEIVE_NEWS } from '../actions/news_actions'
import { LOGOUT_CURRENT_USER } from '../actions/session_actions'

export default (state = [], action) => {
  Object.freeze(state);
  switch (action.type) {
    case RECEIVE_NEWS: 
      return [].concat(state).concat([action.news])
    case LOGOUT_CURRENT_USER: 
      return []
    default: 
    return state;
  }
}