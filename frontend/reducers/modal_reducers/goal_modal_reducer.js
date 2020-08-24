import {
  OPEN_MODAL,
  CLOSE_MODAL
} from '../../actions/modal_actions'

export default function goalModalReducer(state = [], action) {
  switch (action.type) {
    case OPEN_MODAL:
      if (action.payload.goal_category) {
        return [action.payload];
      }
      case CLOSE_MODAL:
        return [];
      default:
        return state;
  }
}