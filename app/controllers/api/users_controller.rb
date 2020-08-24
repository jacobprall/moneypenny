class Api::UsersController < ApplicationController

  def create  
    @user = User.new(user_params)

    if @user.save
      @account = Account.create(debit: true, account_category: 'Cash', institution: 'None', label: 'Greenbacks', balance: 0, user_id: @user.id)
      @transaction = Transaction.create(amount: 0, date: DateTime.new, description: 'My First Transaction', transaction_category: 'Miscellaneous', account_id: @account.id)
      login!(@user)
      render 'api/users/show'
    else
      render json: @user.errors.full_messages, status: 422
    end
  end

  def user_params
    params.require(:user).permit(:email, :p_num, :password)
  end
end
