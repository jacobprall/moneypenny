import React from 'react';
import { closeModal } from '../actions/modal_actions';
import { connect } from 'react-redux';
import AccountNewContainer from './accounts/account_form_modals/account_new_container';
import AccountEditContainer from './accounts/account_form_modals/account_edit_container';
import CreateTransactionContainer from './transactions/transaction_form/create_transaction_container'
import EditTransactionContainer from './transactions/transaction_form/edit_transaction_container'
function Modal({ modal, closeModal }) {
  
  if (!modal.account && !modal.transaction) {
    return null;
  }

  let component;

  if (modal.account !== null) {
    switch (modal.account[0]) {
      
      case 'new account':
        component = <AccountNewContainer />;
        break;
      case 'edit account':
        component = <AccountEditContainer />;
        break;
      default:
        return null;
    }
  } else if (modal.transaction !== null) {
    switch (modal.transaction[0]) {
      case 'new transaction':
        component = <CreateTransactionContainer />
      case 'edit transaction':
        component = <EditTransactionContainer />
      default:
        return null;
    }
  }

  return (
    <div className="modal-background" onClick={closeModal}>
      <div className="modal-child" onClick={e => e.stopPropagation()}>
        {component}
      </div>
    </div>
  );
}



const mapStateToProps = state => {
  return {
    modal: state.ui.modal
  };
};

const mapDispatchToProps = dispatch => {
  return {
    closeModal: () => dispatch(closeModal())
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(Modal);