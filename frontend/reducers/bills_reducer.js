import { RECEIVE_BILLS, RECEIVE_BILL, REMOVE_BILL } from '../actions/bill_actions'

const billsReducer = (oldState = {}, action) => {
  let newState = Object.assign({}, oldState);

  switch (action.type) {
    case RECEIVE_BILLS:
      newState = action.bills;
      return newState;
    case RECEIVE_BILL:
      let newBill = {[action.bill.id]: action.bill};
      newState = Object.assign(newState, newBill)
      return newState;
    case REMOVE_BILL:
      if (action.bill.recurring) {
        let newBill = {[action.bill.id]: action.bill};
        return Object.assign(newState, newBill);
      } else {
        delete newState[action.bill.id]
        return newState
      }
    default:
      return newState;
  }
}

export default billsReducer