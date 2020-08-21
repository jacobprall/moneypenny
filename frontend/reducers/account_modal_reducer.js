import { OPEN_MODAL, CLOSE_MODAL } from '../actions/modal_actions'

export default function accountModalReducer(state = null, action) {
  switch (action.type) {
    case OPEN_MODAL:
      if (action.modalType.split(' ')[1] === 'account') {
        return [action.modalType, action.payload];
      }
    case CLOSE_MODAL:
      return null;
    default:
      return state;
  }
}