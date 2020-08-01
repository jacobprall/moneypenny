# == Schema Information
#
# Table name: transactions
#
#  id          :bigint           not null, primary key
#  amount      :decimal(8, 2)    not null
#  category    :string
#  date        :datetime         not null
#  description :string
#  notes       :text
#  created_at  :datetime         not null
#  updated_at  :datetime         not null
#  account_id  :integer          not null
#
# Indexes
#
#  index_transactions_on_account_id  (account_id)
#
require 'test_helper'

class TransactionTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
