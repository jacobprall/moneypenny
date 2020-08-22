
import { OPEN_MODAL, CLOSE_MODAL } from '../../actions/modal_actions'

export default function componentModalReducer(state = [], action) {
  switch (action.type) {
    case OPEN_MODAL:
      return [action.component];
    case CLOSE_MODAL:
      return [];
    default:
      return state;
  }
};