import { OPEN_MODAL, CLOSE_MODAL } from '../actions/modal_actions'

export default function accountModalReducer(state = null, action) {
  switch (action.type) {
    case OPEN_MODAL: 
      return [action.modalType, action.account];
    case CLOSE_MODAL:
      return null;
    default:
      return state;
  }
}